use std::{path::PathBuf, str::FromStr};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use color_eyre::eyre::{bail, eyre};
use html2md_bulletty::parse_html;
use regex::Regex;
use reqwest::blocking::Client;
use roxmltree::Node;
use slug::slugify;
use tracing::error;
use url::Url;

use crate::core::{
    feed::{feedentry::FeedEntry, feedutils, html},
    library::feeditem::FeedItem,
};

pub fn get_feed_with_data(url: &str) -> color_eyre::Result<(FeedItem, String)> {
    let client = Client::builder()
        .user_agent(format!("bulletty/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    let response = client.get(url).send()?;

    if !response.status().is_success() {
        return Err(eyre!(
            "Request to \"{}\" returned status code {:?}",
            url,
            response.status()
        ));
    }

    let body = response.text()?;

    // If the response is HTML try to follow metadata feed links
    if html::is_html(&body) {
        let url = Url::from_str(url)?; // Fails with same error as the reqwest send() above
        let link_parser = match html::LinkParser::new(&body, &url) {
            Ok(p) => p,
            Err(html::ParseError::TooLarge) => {
                bail!("HTML page at \"{}\" is too large to parse", url);
            }
        };

        return link_parser
            .into_iter()
            .take(3)
            .find_map(|feed_url| get_feed_with_data(&feed_url).ok())
            .ok_or_else(|| eyre!("No embedded RSS/Atom feed links found at \"{url}\""));
    }

    Ok((parse(&body, url)?, body))
}

pub fn get_feed(url: &str) -> color_eyre::Result<FeedItem> {
    let (feeditem, _) = get_feed_with_data(url)?;
    Ok(feeditem)
}

fn extract_title(node: &Node) -> String {
    node.descendants()
        .find(|t| t.tag_name().name() == "title")
        .and_then(|t| {
            let text = t.text()?;
            if t.attribute("type") == Some("html") {
                Some(parse_html(text))
            } else {
                Some(text.to_string())
            }
        })
        .map(|s| feedutils::normalize_and_truncate(s, 256))
        .unwrap_or_default()
}

fn parse(doc: &str, feed_url: &str) -> color_eyre::Result<FeedItem> {
    let mut feed = FeedItem::default();

    let doc = roxmltree::Document::parse(doc)?;
    let feed_tag = doc.root();

    feed.title = extract_title(&feed_tag);

    feed.description = feed_tag
        .descendants()
        .find(|t| t.tag_name().name() == "description" || t.tag_name().name() == "subtitle")
        .and_then(|t| t.text())
        .unwrap_or(&feed.title)
        .to_string();

    feed.url = feed_tag
        .descendants()
        .find(|t| t.tag_name().name() == "link")
        .and_then(|t| {
            if t.text().is_none() {
                t.attribute("href")
            } else {
                t.text()
            }
        })
        .unwrap_or(feed_url)
        .to_string();

    feed.feed_url = feed_url.to_string();

    if let Some(author_tag) = feed_tag
        .descendants()
        .find(|t| t.tag_name().name() == "author")
    {
        if let Some(nametag) = author_tag
            .descendants()
            .find(|t| t.tag_name().name() == "name")
            .and_then(|t| t.text())
        {
            feed.author = String::from(nametag);
        } else if let Some(text) = author_tag.text() {
            feed.author = String::from(text);
        } else {
            feed.author = feed.title.to_string();
        }
    } else {
        feed.author = feed.title.to_string();
    }

    feed.slug = slugify(&feed.title);

    Ok(feed)
}

pub fn get_feed_entries(feed: &FeedItem) -> color_eyre::Result<Vec<FeedEntry>> {
    let client = Client::builder()
        .user_agent(format!("bulletty/{}", env!("CARGO_PKG_VERSION")))
        .build()?;

    let response = client.get(&feed.feed_url).send()?;

    if !response.status().is_success() {
        return Err(eyre!(
            "Request to \"{}\" returned status code {:?}",
            feed.feed_url,
            response.status()
        ));
    }

    let body = response.text()?;
    get_feed_entries_doc(&body, &feed.author)
}

pub fn get_feed_entries_doc(
    doctxt: &str,
    defaultauthor: &str,
) -> color_eyre::Result<Vec<FeedEntry>> {
    let doc = roxmltree::Document::parse(doctxt)?;

    let feed_tag = doc.root();

    let mut feedentries = Vec::<FeedEntry>::new();

    for entry in feed_tag
        .descendants()
        .filter(|t| t.tag_name().name() == "item" || t.tag_name().name() == "entry")
    {
        let (desc, content) = get_description_content(&entry);

        // date extraction
        let datestr = entry
            .descendants()
            .find(|t| {
                t.tag_name().name() == "published"
                    || t.tag_name().name() == "updated"
                    || t.tag_name().name() == "date"
                    || t.tag_name().name() == "pubDate"
            })
            .and_then(|t| t.text())
            .unwrap_or("1990-09-19")
            .to_string();

        // author extraction
        let entryauthor: String = if let Some(author_tag) = entry
            .descendants()
            .find(|t| t.tag_name().name() == "author" || t.tag_name().name() == "creator")
        {
            if let Some(nametag) = author_tag
                .descendants()
                .find(|t| t.tag_name().name() == "name")
                .and_then(|t| t.text())
            {
                String::from(nametag)
            } else if let Some(text) = author_tag.text() {
                String::from(text)
            } else {
                defaultauthor.to_string()
            }
        } else {
            defaultauthor.to_string()
        };

        // url extraction
        let entryurl = entry
            .descendants()
            .find(|t| {
                if t.tag_name().name() == "id"
                    && let Some(text) = t.text()
                    && let Ok(url) = Url::parse(text)
                    && (url.scheme() == "http" || url.scheme() == "https")
                {
                    return true;
                }

                if t.tag_name().name() == "enclosure"
                    && let Some(text) = t.attribute("url")
                    && let Ok(url) = Url::parse(text)
                    && (url.scheme() == "http" || url.scheme() == "https")
                {
                    return true;
                }

                t.tag_name().name() == "link"
            })
            .and_then(|t| {
                if t.text().is_none() {
                    if t.attribute("url").is_some() {
                        return t.attribute("url");
                    }

                    t.attribute("href")
                } else {
                    t.text()
                }
            })
            .unwrap_or("NOURL")
            .to_string();

        // feed creation
        let fe = FeedEntry {
            title: extract_title(&entry),
            author: entryauthor,
            url: entryurl.clone(),
            text: content,
            date: parse_date(&datestr)
                .map_err(|err| error!("{:?} from {entryurl}", err))
                .unwrap_or_default(),
            description: desc,
            lastupdated: Utc::now(),
            seen: false,
            filepath: PathBuf::default(),
        };

        feedentries.push(fe);
    }

    Ok(feedentries)
}

fn parse_date(date_str: &str) -> color_eyre::Result<DateTime<Utc>> {
    let mut errors = Vec::new();

    // Attempt to parse as RFC3339 (e.g., "2024-01-01T12:00:00Z" or "2024-01-01T12:00:00+01:00")
    match DateTime::parse_from_rfc3339(date_str) {
        Ok(dt) => return Ok(dt.with_timezone(&Utc)),
        Err(e) => {
            if e.kind() != chrono::format::ParseErrorKind::Invalid {
                errors.push(format!("RFC3339: {e}, {:?}", e.kind()));
            }
        }
    }

    // Attempt to parse as RFC2822 (e.g., "Mon, 01 Jan 2024 12:00:00 +0000")
    match DateTime::parse_from_rfc2822(date_str) {
        Ok(dt) => return Ok(dt.with_timezone(&Utc)),
        Err(e) => {
            if e.kind() != chrono::format::ParseErrorKind::Invalid {
                errors.push(format!("RFC2822: {e}, {:?}", e.kind()));
            }
        }
    }

    // Attempt to parse a NaiveDateTime with no offset (e.g., "2024-01-01 12:00:00")
    let format_naive_datetime = "%Y-%m-%d %H:%M:%S";
    match NaiveDateTime::parse_from_str(date_str, format_naive_datetime) {
        Ok(naive) => return Ok(DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc)),
        Err(e) => {
            if e.kind() != chrono::format::ParseErrorKind::Invalid {
                errors.push(format!(
                    "NaiveDateTime ('{format_naive_datetime}'): {e}, {:?}",
                    e.kind()
                ));
            }
        }
    }

    // Attempt to parse RFC2822-style without weekday and timezone
    // e.g. "Sun, 31 August 2025 07:00:00 GMT" -> "31 August 2025 07:00:00"
    let parts: Vec<&str> = date_str.split_whitespace().collect();
    if parts.len() >= 5 {
        let clean_date_str = parts[1..5].join(" ");
        let format_clean = "%d %B %Y %H:%M:%S";
        match NaiveDateTime::parse_from_str(&clean_date_str, format_clean) {
            Ok(dt) => return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc)),
            Err(e) => {
                if e.kind() != chrono::format::ParseErrorKind::Invalid {
                    errors.push(format!(
                        "Cleaned Date ('{clean_date_str}' with '{format_clean}'): {e}, {:?}",
                        e.kind()
                    ));
                }
            }
        }
    }

    // Attempt to parse a NaiveDate (e.g., "2024-01-01") and set time to midnight UTC
    let format_naive_date = "%Y-%m-%d";
    match NaiveDate::parse_from_str(date_str, format_naive_date) {
        Ok(naive_date) => {
            if let Some(naive_datetime) = naive_date.and_hms_opt(0, 0, 0) {
                return Ok(DateTime::<Utc>::from_naive_utc_and_offset(
                    naive_datetime,
                    Utc,
                ));
            }
            errors.push(format!("NaiveDate ('{format_naive_date}'): invalid time"));
        }
        Err(e) => {
            if e.kind() != chrono::format::ParseErrorKind::Invalid {
                errors.push(format!(
                    "NaiveDate ('{format_naive_date}'): {e}, {:?}",
                    e.kind()
                ));
            }
        }
    }

    Err(eyre!(
        "Couldn't parse date: {:?}. Possible errors: {:#?}",
        date_str,
        errors
    ))
}

fn get_description_content(entry: &Node) -> (String, String) {
    let content = entry
        .descendants()
        .find(|t| t.tag_name().name() == "content" || t.tag_name().name() == "encoded")
        .and_then(|t| t.text());

    let description = entry
        .descendants()
        .find(|t| t.tag_name().name() == "description" || t.tag_name().name() == "summary")
        .and_then(|t| t.text());

    let content_text = match content.as_ref() {
        Some(text) => parse_html(text),
        None => match description.as_ref() {
            Some(desc) => parse_html(desc),
            None => String::new(),
        },
    };

    let description_text = match description {
        Some(text) => parse_html(text)
            .replace("\n", "")
            .chars()
            .take(280)
            .collect::<String>(),
        None => content_text
            .replace("\n", "")
            .chars()
            .take(280)
            .collect::<String>(),
    };

    (strip_markdown_tags(&description_text), content_text)
}

fn strip_markdown_tags(input: &str) -> String {
    let patterns = [
        r"\*\*(.*?)\*\*",     // bold **
        r"\*(.*?)\*",         // italic *
        r"`(.*?)`",           // inline code
        r"~~(.*?)~~",         // strikethrough
        r"#+\s*",             // headings
        r"!\[(.*?)\]\(.*?\)", // images
        r"\[(.*?)\]\(.*?\)",  // links
        r">+\s*",             // blockquotes
        r"[-*_=]{3,}",        // horizontal rules
        r"`{3}.*?`{3}",       // code blocks
    ];
    let mut result = input.to_string();
    for pat in patterns.iter() {
        let re = Regex::new(pat).unwrap();
        result = re.replace_all(&result, "$1").to_string();
    }
    result
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn test_strip_markdown_tags() {
        let input = "**bold** *italic* `code` ~~strike~~ [link](url) ![image](url) # heading > blockquote\n---\n";
        let expected = "bold italic code strike link image heading blockquote\n\n";
        assert_eq!(strip_markdown_tags(input), expected);
    }

    #[test]
    fn test_parse_date_various_formats() {
        let datetime_strings = [
            "2024-01-01T12:00:00Z",             // RFC3339 UTC
            "2024-01-01T13:00:00+01:00",        // RFC3339 with offset
            "2024-02-29 09:00:00",              // Naive datetime
            "2023-11-20",                       // Naive date
            "Mon, 01 Jan 2024 12:00:00 +0000",  // RFC2822
            "Sun, 31 August 2025 07:00:00 GMT", // Full Month
            "Wed, 02 May 2025 07:00:00 GMT",    // Impossible, as 2025-05-02 was a Friday
            "Invalid Date String",              // Invalid format
        ];

        let expected = [
            Some(
                DateTime::parse_from_rfc3339("2024-01-01T12:00:00+00:00")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            Some(
                DateTime::parse_from_rfc3339("2024-01-01T12:00:00+00:00")
                    .unwrap()
                    .with_timezone(&Utc),
            ), // 13:00+01:00 == 12:00Z
            Some(Utc.with_ymd_and_hms(2024, 2, 29, 9, 0, 0).unwrap()),
            Some(Utc.with_ymd_and_hms(2023, 11, 20, 0, 0, 0).unwrap()),
            Some(Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap()),
            Some(Utc.with_ymd_and_hms(2025, 8, 31, 7, 0, 0).unwrap()),
            Some(Utc.with_ymd_and_hms(2025, 5, 2, 7, 0, 0).unwrap()),
            None,
        ];

        for (input, expected_str) in datetime_strings.iter().zip(expected.iter()) {
            let result = parse_date(input);
            match expected_str {
                Some(exp) => match result {
                    Ok(ref dt) => assert_eq!(dt, exp, "Failed on input: {input}"),
                    Err(e) => panic!("Expected Ok for input: {input} - Error: {e}"),
                },
                None => assert!(result.is_err(), "Expected error for input: {input}"),
            }
        }
    }

    #[test]
    fn parses_rss2_channel_fields() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Example RSS</title>
    <link>https://example.com/</link>
    <description>RSS description</description>
    <author>Alice</author>
    <item>
      <title>Item 1</title>
      <link>https://example.com/item1</link>
      <description>Item 1 description</description>
      <author>alice@example.com (Alice)</author>
    </item>
  </channel>
</rss>"#;

        let feed = parse(xml, "NOURL").expect("failed to parse RSS 2.0");
        assert_eq!(feed.title, "Example RSS");
        assert_eq!(feed.description, "RSS description");
        assert_eq!(feed.url, "https://example.com/");
        assert!(feed.author.contains("Alice"));
    }

    #[test]
    fn parses_atom_feed_fields() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Example Atom</title>
  <subtitle>Atom description</subtitle>
  <link href="https://example.org/"/>
  <author>
    <name>Bob</name>
  </author>
  <id>urn:uuid:60a76c80-d399-11d9-b93C-0003939e0af6</id>
  <updated>2003-12-13T18:30:02Z</updated>
</feed>"#;

        let feed = parse(xml, "NOURL").expect("failed to parse Atom");
        assert_eq!(feed.title, "Example Atom");
        assert_eq!(feed.description, "Atom description");
        assert_eq!(feed.url, "https://example.org/");
        assert_eq!(feed.author, "Bob");
    }

    #[test]
    fn rss_missing_link_uses_default_url() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>No Link RSS</title>
    <description>No link here</description>
    <author>Carol</author>
  </channel>
</rss>"#;

        let feed = parse(xml, "NOURL").expect("failed to parse RSS without link");
        assert_eq!(feed.title, "No Link RSS");
        assert_eq!(feed.description, "No link here");
        assert_eq!(feed.url, "NOURL");
        assert!(feed.author.contains("Carol"));
    }

    #[test]
    fn rss_missing_author_uses_feed_title() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>No Author RSS</title>
    <description>No author here</description>
  </channel>
</rss>"#;

        let feed = parse(xml, "NOURL").expect("failed to parse RSS without author");
        assert_eq!(feed.title, "No Author RSS");
        assert_eq!(feed.description, "No author here");
        assert_eq!(feed.author, "No Author RSS");
    }

    #[test]
    fn get_feed_entries_doc_parses_rss_items_variants() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
 <rss version="2.0" xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/">
   <channel>
     <title>Example RSS</title>
     <link>https://example.com/</link>
     <description>RSS description</description>
     <author>Carol</author>
     <item>
       <title>Item A</title>
       <link>https://example.com/a</link>
       <description>Item A description</description>
       <pubDate>Mon, 01 Jan 2024 12:00:00 +0000</pubDate>
       <content:encoded>Item A content</content:encoded>
     </item>
     <item>
       <title>Item B</title>
       <id>https://example.com/b</id>
       <dc:date>2024-03-10T09:30:00Z</dc:date>
       <description>Item B description</description>
     </item>
   </channel>
 </rss>"#;

        let entries = get_feed_entries_doc(xml, "Carol").expect("failed to parse RSS entries");
        assert_eq!(entries.len(), 2);

        // Item A: prefers content:encoded for text, description for description, channel-level author
        let a = &entries[0];
        assert_eq!(a.title, "Item A");
        assert_eq!(a.url, "https://example.com/a");
        assert_eq!(a.author, "Carol");
        assert_eq!(a.text, "Item A content");
        assert_eq!(a.description, "Item A description");
        let expected_a_date = parse_date("Mon, 01 Jan 2024 12:00:00 +0000").unwrap();
        assert_eq!(a.date, expected_a_date);

        // Item B: no content:encoded, uses description for both text and description, dc:date supported
        let b = &entries[1];
        assert_eq!(b.title, "Item B");
        assert_eq!(b.url, "https://example.com/b");
        assert_eq!(b.author, "Carol");
        assert_eq!(b.text, "Item B description");
        assert_eq!(b.description, "Item B description");
        let expected_b_date = DateTime::parse_from_rfc3339("2024-03-10T09:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(b.date, expected_b_date);
    }

    #[test]
    fn get_feed_entries_doc_parses_atom_entries_variants() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
 <feed xmlns="http://www.w3.org/2005/Atom">
   <title>Example Atom</title>
   <link href="https://example.org/"/>
   <author>
     <name>Bob</name>
   </author>
   <id>urn:uuid:feedid</id>
   <updated>2024-01-01T00:00:00Z</updated>
   <entry>
     <title>Entry 1</title>
     <id>https://example.org/e1</id>
     <summary>Summary 1</summary>
     <content>Entry 1 content</content>
     <published>2024-02-01T10:00:00Z</published>
   </entry>
   <entry>
     <title>Entry 2</title>
     <id>https://example.org/e2</id>
     <content>Entry 2 content</content>
     <updated>2024-02-05T11:30:00Z</updated>
     <author>
       <name>Alice</name>
     </author>
   </entry>
   <entry>
     <title>Entry 3</title>
     <link rel="alternate" href="https://example.org/e3" type="text/html"/>
     <id>https://example.org/e3</id>
     <content>Entry 3 content</content>
     <updated>2024-02-05T11:30:00Z</updated>
     <author>
       <name>Alice</name>
     </author>
   </entry>
 </feed>"#;

        let entries = get_feed_entries_doc(xml, "Bob").expect("failed to parse Atom entries");
        assert_eq!(entries.len(), 3);

        // Entry 1: uses summary for description, content for text, published for date, id for URL, feed-level author
        let e1 = &entries[0];
        assert_eq!(e1.title, "Entry 1");
        assert_eq!(e1.url, "https://example.org/e1");
        assert_eq!(e1.author, "Bob");
        assert_eq!(e1.text, "Entry 1 content");
        assert_eq!(e1.description, "Summary 1");
        let expected_e1_date = DateTime::parse_from_rfc3339("2024-02-01T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(e1.date, expected_e1_date);

        // Entry 2: no summary -> description falls back to content, updated for date, id for URL
        let e2 = &entries[1];
        assert_eq!(e2.title, "Entry 2");
        assert_eq!(e2.url, "https://example.org/e2");
        assert_eq!(e2.author, "Alice");
        assert_eq!(e2.text, "Entry 2 content");
        assert_eq!(e2.description, "Entry 2 content");
        let expected_e2_date = DateTime::parse_from_rfc3339("2024-02-05T11:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(e2.date, expected_e2_date);

        // Entry 3: both link tags
        let e3 = &entries[2];
        assert_eq!(e3.title, "Entry 3");
        assert_eq!(e3.url, "https://example.org/e3");
        assert_eq!(e3.author, "Alice");
        assert_eq!(e3.text, "Entry 3 content");
        assert_eq!(e3.description, "Entry 3 content");
        let expected_e3_date = DateTime::parse_from_rfc3339("2024-02-05T11:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(e3.date, expected_e3_date);
    }

    #[test]
    fn get_feed_entries_doc_parses_atom_entry_level_author_overrides_feed() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Example Atom</title>
  <link href="https://example.org/"/>
  <author>
    <name>Feed Author</name>
  </author>
  <id>urn:uuid:feedid</id>
  <updated>2024-01-01T00:00:00Z</updated>

  <entry>
    <title>Entry Has Own Author</title>
    <id>https://example.org/own</id>
    <author>
      <name>Alice</name>
    </author>
    <content>Own author content</content>
    <published>2024-02-01T10:00:00Z</published>
  </entry>

  <entry>
    <title>Entry Falls Back To Feed Author</title>
    <id>https://example.org/fallback</id>
    <content>No entry author here</content>
    <updated>2024-02-05T11:30:00Z</updated>
  </entry>
</feed>"#;

        let entries = get_feed_entries_doc(xml, "Feed Author")
            .expect("failed to parse Atom entries with entry-level authors");
        assert_eq!(entries.len(), 2);

        let e1 = &entries[0];
        assert_eq!(e1.title, "Entry Has Own Author");
        assert_eq!(e1.url, "https://example.org/own");
        assert_eq!(e1.author, "Alice"); // entry-level author should override feed-level author

        let e2 = &entries[1];
        assert_eq!(e2.title, "Entry Falls Back To Feed Author");
        assert_eq!(e2.url, "https://example.org/fallback");
        assert_eq!(e2.author, "Feed Author"); // falls back to feed-level author
    }

    #[test]
    fn get_feed_entries_doc_parses_rss_item_level_author_overrides_channel() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>Example RSS</title>
    <link>https://example.com/</link>
    <description>RSS description</description>
    <author>Channel Author</author>
    <item>
      <title>Item With Author</title>
      <link>https://example.com/with-author</link>
      <description>Has its own author</description>
      <author>Alice</author>
      <pubDate>Mon, 01 Jan 2024 12:00:00 +0000</pubDate>
    </item>
    <item>
      <title>Item With DC Creator</title>
      <link>https://example.com/with-dc-creator</link>
      <description>Has dc:creator</description>
      <dc:creator>Dave</dc:creator>
      <dc:date>2024-02-01T10:00:00Z</dc:date>
    </item>
  </channel>
</rss>"#;

        let entries = get_feed_entries_doc(xml, "Channel Author")
            .expect("failed to parse RSS entries with entry-level authors");
        assert_eq!(entries.len(), 2);

        let a = &entries[0];
        assert_eq!(a.title, "Item With Author");
        assert_eq!(a.url, "https://example.com/with-author");
        assert_eq!(a.author, "Alice"); // item-level <author> should override channel author

        let b = &entries[1];
        assert_eq!(b.title, "Item With DC Creator");
        assert_eq!(b.url, "https://example.com/with-dc-creator");
        assert_eq!(b.author, "Dave"); // entry-level <dc:creator> should override channel author
    }

    #[test]
    fn get_feed_entries_youtube_style() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns:yt="http://www.youtube.com/xml/schemas/2015" xmlns:media="http://search.yahoo.com/mrss/" xmlns="http://www.w3.org/2005/Atom">
 <link rel="self" href="http://www.youtube.com/feeds/videos.xml?channel_id=channelID"/>
 <id>yt:channel:rqM0Ym_NbK1fqeQG2VIohg</id>
 <yt:channelId>rqM0Ym_NbK1fqeQG2VIohg</yt:channelId>
 <title>Some Youtube Channel</title>
 <link rel="alternate" href="https://www.youtube.com/channel/SomeYoutubeChannel"/>
 <author>
  <name>Some Youtube Author</name>
  <uri>https://www.youtube.com/channel/SomeYoutubeChannel</uri>
 </author>
 <published>2019-01-12T00:02:33+00:00</published>
 <entry>
  <id>yt:video:videoId</id>
  <yt:videoId>videoId</yt:videoId>
  <yt:channelId>channelId</yt:channelId>
  <title>Video Title</title>
  <link rel="alternate" href="https://www.youtube.com/watch?v=VIDEOID"/>
  <author>
   <name>Some Youtube Author</name>
   <uri>https://www.youtube.com/channel/SomeYoutubeChannel</uri>
  </author>
  <published>2025-12-05T13:34:36+00:00</published>
  <updated>2025-12-05T17:01:19+00:00</updated>
  <media:group>
   <media:title>Video Title</media:title>
   <media:content url="https://www.youtube.com/v/VIDEOID?version=3" type="application/x-shockwave-flash" width="640" height="390"/>
   <media:thumbnail url="https://i2.ytimg.com/vi/VIDEOID/hqdefault.jpg" width="480" height="360"/>
   <media:description>This is a description!</media:description>
   <media:community>
    <media:starRating count="1012" average="5.00" min="1" max="5"/>
    <media:statistics views="30006"/>
   </media:community>
  </media:group>
 </entry> 
</feed>"#;

        let entries = get_feed_entries_doc(xml, "Channel Author")
            .expect("failed to parse feed youtube style");
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];

        assert_eq!(entry.title, "Video Title");
        assert_eq!(entry.url, "https://www.youtube.com/watch?v=VIDEOID");
        assert_eq!(entry.author, "Some Youtube Author");
        assert_eq!(entry.description, "This is a description!");
    }

    #[test]
    fn get_feed_entries_doc_handles_html_entities_in_titles() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>HTML Entity Test</title>
    <link>https://example.com/</link>
    <description>Testing HTML entity decoding in entry titles</description>
    <item>
      <title>&#8220;curly&#8221; &amp; &lt;tag&gt;</title>
      <link>https://example.com/plain</link>
    </item>
    <item>
      <title type="html"><![CDATA[ &#8220;curly&#8221; &amp; &lt;tag&gt; ]]></title>
      <link>https://example.com/cdata-html</link>
    </item>
    <item>
      <title><![CDATA[ &#8220;curly&#8221; &amp; &lt;tag&gt; ]]></title>
      <link>https://example.com/cdata-plain</link>
    </item>
  </channel>
</rss>"#;

        let entries = get_feed_entries_doc(xml, "Author")
            .expect("failed to parse RSS entries with HTML entities");
        assert_eq!(entries.len(), 3);

        // Plain title: numeric and predefined entities are decoded.
        assert_eq!(entries[0].title, "“curly” & <tag>");

        // CDATA with type="html": entities inside CDATA are decoded and the
        // content is converted from HTML to Markdown, so stray `<tag>` is escaped.
        assert_eq!(entries[1].title, "“curly” & \\<tag\\>");

        // CDATA without type="html": content stays literal.
        assert_eq!(entries[2].title, "&#8220;curly&#8221; &amp; &lt;tag&gt;");
    }

    #[test]
    fn get_feed_entries_doc_converts_html_tags_to_markdown_in_cdata_html_titles() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>HTML to Markdown Test</title>
    <link>https://example.com/</link>
    <description>Testing HTML tag conversion in CDATA html titles</description>
    <item>
      <title type="html"><![CDATA[ <b>bold</b> and <a href="https://example.com">a link</a> ]]></title>
      <link>https://example.com/1</link>
    </item>
  </channel>
</rss>"#;

        let entries =
            get_feed_entries_doc(xml, "Author").expect("failed to parse RSS entry with HTML title");
        assert_eq!(entries.len(), 1);

        assert_eq!(
            entries[0].title,
            "**bold** and [a link](https://example.com)"
        );
    }

    #[test]
    fn get_feed_entries_podcast_style() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:itunes="http://www.itunes.com/dtds/podcast-1.0.dtd" xmlns:googleplay="http://www.google.com/schemas/play-podcasts/1.0" xmlns:atom="http://www.w3.org/2005/Atom" xmlns:media="http://search.yahoo.com/mrss/" xmlns:content="http://purl.org/rss/1.0/modules/content/">
  <channel>
    <atom:link href="https://podcast_link.com" rel="self" type="application/rss+xml"/>
    <title>Podcast Title</title>
    <link>https://podcast_link.com</link>
    <item>
      <title>Podcast Entry Title</title>
      <description>Podcast Entry Description</description>
      <pubDate>Fri, 05 Dec 2025 18:27:00 -0000</pubDate>
      <itunes:episodeType>full</itunes:episodeType>
      <itunes:author>Podcast Author</itunes:author>
      <itunes:image href="https://podcast_link.com/thumbnail"/>
      <itunes:subtitle></itunes:subtitle>
      <itunes:summary>Podcast Entry Description #1</itunes:summary>
      <content:encoded>Podcast Content Data</content:encoded>
      <itunes:duration>4634</itunes:duration>
      <itunes:explicit>no</itunes:explicit>
      <guid isPermaLink="false"><![CDATA[1bae995c-d208-11f0-8bf7-cb6936959f42]]></guid>
      <enclosure url="https://podcast_link.com/audio" length="0" type="audio/mpeg"/>
    </item>
  </channel>
</rss>"#;

        let entries = get_feed_entries_doc(xml, "Channel Author")
            .expect("failed to parse feed podcasty style");
        assert_eq!(entries.len(), 1);

        let entry = &entries[0];

        assert_eq!(entry.title, "Podcast Entry Title");
        assert_eq!(entry.url, "https://podcast_link.com/audio");
        assert_eq!(entry.author, "Podcast Author");
        assert_eq!(entry.description, "Podcast Entry Description");
    }
}
