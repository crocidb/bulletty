use std::io::Write;
use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
};

use chrono::{Duration, Utc};
use color_eyre::eyre::eyre;
use slug::slugify;
use tracing::{error, info};

use crate::core::feed::feedentry::FeedEntry;
use crate::core::feed::feedparser;
use crate::core::library::feedcategory::FeedCategory;
use crate::{
    core::defs::{self, DATA_CATEGORIES_DIR, DATA_FEED, DATA_READ_LATER},
    core::library::feeditem::FeedItem,
};
use serde::{Deserialize, Serialize};

#[cfg(test)]
use tempfile::TempDir;

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct ReadLaterData {
    pub read_later: Vec<String>,
    #[serde(skip_serializing, skip_deserializing)]
    pub loaded: bool,
}

pub struct LibraryData {
    pub path: PathBuf,
    pub read_later: ReadLaterData,
}

impl LibraryData {
    pub fn new(datapath: &Path) -> LibraryData {
        load_or_create(datapath);
        LibraryData {
            path: PathBuf::from(datapath),
            read_later: ReadLaterData::default(),
        }
    }

    #[cfg(test)]
    pub fn new_for_test() -> (LibraryData, TempDir) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().to_path_buf();
        load_or_create(&path);
        (
            LibraryData {
                path,
                read_later: ReadLaterData::default(),
            },
            temp_dir,
        )
    }

    pub fn feed_exists(&self, slug: &str, category: &str) -> bool {
        let feeddata = self
            .path
            .join(DATA_CATEGORIES_DIR)
            .join(category)
            .join(slug)
            .join(DATA_FEED);
        feeddata.exists()
    }
    pub fn delete_feed(&self, slug: &str, category: &str) -> color_eyre::Result<()> {
        let feed_dir = self
            .path
            .join(DATA_CATEGORIES_DIR)
            .join(category)
            .join(slug);

        if feed_dir.exists() {
            fs::remove_dir_all(&feed_dir).map_err(|e| {
                eyre!(
                    "Failed to delete feed directory {}: {}",
                    feed_dir.display(),
                    e
                )
            })
        } else {
            Ok(()) // Nothing to delete
        }
    }

    pub fn feed_create(&self, feed: &FeedItem) -> color_eyre::Result<()> {
        let feedir = self
            .path
            .join(DATA_CATEGORIES_DIR)
            .join(&feed.category)
            .join(&feed.slug);
        fs::create_dir_all(&feedir)?;

        let feeddata = feedir.join(DATA_FEED);
        let toml_str =
            toml::to_string(feed).map_err(|e| eyre!("Failed to serialize feed: {}", e))?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&feeddata)
            .map_err(|e| eyre!("Couldn't open file {}: {}", feeddata.display(), e))?;

        file.write_all(toml_str.as_bytes())
            .map_err(|e| eyre!("Failed to write file {}: {}", feeddata.display(), e))
    }

    pub fn generate_categories_tree(&self) -> color_eyre::Result<Vec<FeedCategory>> {
        let mut categories: Vec<FeedCategory> = Vec::new();
        let catpath = self.path.join(DATA_CATEGORIES_DIR);

        for entry in fs::read_dir(catpath)? {
            let path = entry?.path();
            if path.is_dir()
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
            {
                let cat = FeedCategory {
                    title: String::from(name),
                    feeds: self.load_feeds_from_category(name, path.as_path())?,
                };

                categories.push(cat);
            }
        }

        categories.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(categories)
    }

    pub fn load_feeds_from_category(
        &self,
        category_name: &str,
        category: &Path,
    ) -> color_eyre::Result<Vec<FeedItem>> {
        let mut feeds = Vec::new();

        for entry in fs::read_dir(category)? {
            let path = entry?.path();
            if path.is_dir() {
                let feedpath = path.join(defs::DATA_FEED);

                if let Ok(file) = std::fs::read_to_string(&feedpath) {
                    let mut feed: FeedItem = match toml::from_str(&file) {
                        Ok(f) => f,
                        Err(e) => {
                            return Err(eyre!("Error: feed file can't be parsed: {}", e));
                        }
                    };

                    feed.category = category_name.to_string();
                    feeds.push(feed);
                }
            }
        }

        feeds.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        Ok(feeds)
    }

    pub fn update_feed_entries(
        &self,
        category: &str,
        feed: &FeedItem,
        feedxml: String,
    ) -> color_eyre::Result<()> {
        if !Self::feed_needs_update(feed) {
            return Ok(());
        }

        let feedentries = feedparser::get_feed_entries_doc(&feedxml, &feed.author)?;

        self.apply_feed_update(category, feed, feedentries)
    }

    pub fn feed_needs_update(feed: &FeedItem) -> bool {
        Utc::now().signed_duration_since(feed.lastupdated) >= Duration::minutes(5)
    }

    pub fn apply_feed_update(
        &self,
        category: &str,
        feed: &FeedItem,
        mut feedentries: Vec<FeedEntry>,
    ) -> color_eyre::Result<()> {
        feedentries.iter_mut().for_each(|e| {
            let entrypath = self
                .path
                .join(defs::DATA_CATEGORIES_DIR)
                .join(category)
                .join(&feed.slug);

            let item_slug = {
                let base_path = entrypath.to_string_lossy();
                let max_slug_len = 250usize.saturating_sub(base_path.len() + 1);
                let slug = slugify(&e.title);
                let slug_cut = &slug[..slug.len().min(max_slug_len)];
                slug_cut.to_string()
            };

            e.filepath = entrypath.join(format!("{item_slug}.md"));
        });

        self.update_entries(feed, feedentries)
    }

    fn update_entries(&self, feed: &FeedItem, entries: Vec<FeedEntry>) -> color_eyre::Result<()> {
        for entry in entries.iter().as_ref() {
            // if it exists, it means the entry has been setup already
            if !entry.filepath.exists() {
                let mut file = match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&entry.filepath)
                {
                    Ok(file) => file,
                    Err(error) => {
                        error!(
                            "Error creating file '{}': {}",
                            entry.filepath.display(),
                            error
                        );

                        break;
                    }
                };

                let mut entryclone = (*entry).clone();
                entryclone.text = String::new();

                let entrytext = format!(
                    "+++\n{}+++\n\n{}",
                    toml::to_string(&entryclone).unwrap_or(String::new()),
                    &entry.text
                );

                file.write_all(&entrytext.into_bytes())?;
            }
        }

        let mut feed = feed.clone();
        feed.lastupdated = Utc::now();
        self.feed_create(&feed)?;

        Ok(())
    }

    pub fn save_feed_entry(&self, entry: &FeedEntry) -> color_eyre::Result<()> {
        info!("Saving {:?}", entry.filepath);

        let mut file = match OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&entry.filepath)
        {
            Ok(file) => file,
            Err(error) => {
                return Err(eyre!(
                    "Error creating file '{}': {}",
                    entry.filepath.display(),
                    error
                ));
            }
        };

        let mut entryclone = (*entry).clone();
        entryclone.text = String::new();

        let entrytext = format!(
            "+++\n{}+++\n\n{}",
            toml::to_string(&entryclone).unwrap_or_default(),
            &entry.text,
        );

        file.write_all(&entrytext.into_bytes())?;

        Ok(())
    }

    pub fn load_feed_entries(
        &self,
        category: &FeedCategory,
        item: &FeedItem,
    ) -> color_eyre::Result<Vec<FeedEntry>> {
        let mut entries = vec![];

        let feedir = self
            .path
            .join(DATA_CATEGORIES_DIR)
            .join(&category.title)
            .join(&item.slug);

        for entry in fs::read_dir(&feedir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let contents = std::fs::read_to_string(&path)?;
                if let Ok(entry) = self.parse_feed_entry(&contents, &path) {
                    entries.push(entry);
                }
            }
        }

        Ok(entries)
    }

    // TODO: this needs to be cached and only updated every now and then, since it's beeing pretty
    // intensive now
    pub fn get_unread_feed(&self, category: &str, feed_slug: &str) -> color_eyre::Result<u16> {
        let mut unread: u16 = 0;

        let feedir = self
            .path
            .join(DATA_CATEGORIES_DIR)
            .join(category)
            .join(feed_slug);

        for entry in fs::read_dir(feedir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let contents = std::fs::read_to_string(&path)?;
                if let Ok(entry) = self.parse_feed_entry(&contents, &path)
                    && !entry.seen
                {
                    unread += 1;
                }
            }
        }

        Ok(unread)
    }

    fn parse_feed_entry(&self, contents: &str, path: &Path) -> color_eyre::Result<FeedEntry> {
        let delimiter = if contents.starts_with("---") {
            "---"
        } else {
            "+++"
        };

        let parts: Vec<&str> = contents.split(delimiter).collect();
        if parts.len() < 3 {
            return Err(eyre!("Invalid feed entry format"));
        }

        let mut entry: FeedEntry = toml::from_str(parts[1].trim())?;
        entry.filepath = path.to_path_buf();
        entry.text = parts[2..].join(delimiter).trim().to_string();
        Ok(entry)
    }

    pub fn set_entry_seen(&self, entry: &FeedEntry) {
        if !entry.seen {
            let mut entry = entry.clone();
            entry.seen = true;
            if let Err(e) = self.save_feed_entry(&entry) {
                error!("Couldn't set entry seen: {:?}", e);
            }
        }
    }

    pub fn toggle_entry_seen(&self, entry: &FeedEntry) {
        let mut entry = entry.clone();
        entry.seen = !entry.seen;
        if let Err(e) = self.save_feed_entry(&entry) {
            error!("Couldn't toggle entry seen: {:?}", e);
        }
    }

    pub fn add_to_read_later(&mut self, entry: &FeedEntry) -> color_eyre::Result<()> {
        self.ensure_read_later()?;

        let rel_path =
            self.absolute_path_to_relative_path(entry.filepath.to_str().unwrap_or_default());

        if rel_path.is_empty() {
            return Ok(());
        }

        // check if entry already exits
        if self.read_later.read_later.iter().any(|p| p == &rel_path) {
            return Ok(());
        }

        self.read_later.read_later.push(rel_path);
        self.save_read_later(&self.read_later)?;

        Ok(())
    }

    pub fn remove_from_read_later(&mut self, file_path: &str) -> color_eyre::Result<()> {
        self.ensure_read_later()?;

        let rel_path = self.absolute_path_to_relative_path(file_path);

        self.read_later.read_later.retain(|p| p != &rel_path);
        self.save_read_later(&self.read_later)?;

        Ok(())
    }

    pub fn get_read_later_feed_entries(&mut self) -> color_eyre::Result<Vec<FeedEntry>> {
        let read_later_list = self.load_read_later()?;
        let mut feed_entries: Vec<FeedEntry> = Vec::new();

        for rel in read_later_list.read_later {
            let full_path = self.path.join(DATA_CATEGORIES_DIR).join(rel);
            if let Ok(contents) = std::fs::read_to_string(&full_path)
                && let Ok(fe) = self.parse_feed_entry(&contents, &full_path)
            {
                feed_entries.push(fe);
            }
        }

        Ok(feed_entries.into_iter().rev().collect())
    }

    pub fn is_in_read_later(&mut self, file_path: &str) -> color_eyre::Result<bool> {
        self.ensure_read_later()?;

        let rel_path = self.absolute_path_to_relative_path(file_path);
        Ok(self.read_later.read_later.iter().any(|p| p == &rel_path))
    }

    fn ensure_read_later(&mut self) -> color_eyre::Result<()> {
        if !self.read_later.loaded {
            self.read_later = self.load_read_later()?;
        }

        Ok(())
    }

    fn load_read_later(&mut self) -> color_eyre::Result<ReadLaterData> {
        let read_later_path = self.path.join(DATA_READ_LATER);
        if !read_later_path.exists() {
            return Ok(ReadLaterData::default());
        }

        let contents = std::fs::read_to_string(&read_later_path)?;
        let mut read_later: ReadLaterData = toml::from_str(&contents)
            .map_err(|e| eyre!("Failed to parse read later data: {}", e))?;

        // Cleanup: drop non-existent entries
        let original_len = read_later.read_later.len();
        read_later.read_later.retain(|rel| {
            let full_path = self.path.join(DATA_CATEGORIES_DIR).join(rel);
            full_path.exists()
        });

        read_later.loaded = true;

        if read_later.read_later.len() < original_len {
            let _ = self.save_read_later(&read_later);
        }

        Ok(read_later)
    }

    fn save_read_later(&self, read_later_list: &ReadLaterData) -> color_eyre::Result<()> {
        let read_later_path = self.path.join(DATA_READ_LATER);
        let toml_str = toml::to_string(read_later_list)
            .map_err(|e| eyre!("Failed to serialize read later data: {}", e))?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&read_later_path)
            .map_err(|e| {
                eyre!(
                    "Couldn't open read later file {}: {}",
                    read_later_path.display(),
                    e
                )
            })?;

        file.write_all(toml_str.as_bytes()).map_err(|e| {
            eyre!(
                "Failed to write read later file {}: {}",
                read_later_path.display(),
                e
            )
        })
    }

    fn absolute_path_to_relative_path(&self, file_path: &str) -> String {
        let path = Path::new(file_path);

        let prefix = self.path.join(DATA_CATEGORIES_DIR);

        if let Ok(rel_path) = path.strip_prefix(&prefix) {
            rel_path.to_str().unwrap_or_default().to_string()
        } else {
            String::new()
        }
    }
}

pub fn load_or_create(path: &Path) {
    let datapath = Path::new(path);
    std::fs::create_dir_all(datapath).expect("Error: Failed to create datapath directory");
    std::fs::create_dir_all(datapath.join(defs::DATA_CATEGORIES_DIR))
        .expect("Error: Failed to create datapath directory");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_feed_entry_plus_delimiter() {
        let (ld, _temp) = LibraryData::new_for_test();
        let content = r#"+++
title = "Test Entry"
description = "Description"
url = "http://example.com"
date = "2023-01-01T00:00:00Z"
lastupdated = "2023-01-01T00:00:00Z"
author = "John Doe"
text = ""
seen = false
+++

This is the content."#;
        let path = Path::new("test.md");

        let result = ld.parse_feed_entry(content, path).unwrap();

        assert_eq!(result.filepath, PathBuf::from("test.md"));
        assert_eq!(result.text, "This is the content.");
        assert_eq!(result.title, "Test Entry");
    }

    #[test]
    fn test_parse_feed_entry_dash_delimiter() {
        let (ld, _temp) = LibraryData::new_for_test();
        let content = r#"---
title = "Test Entry"
description = "Description"
url = "http://example.com"
date = "2023-01-01T00:00:00Z"
lastupdated = "2023-01-01T00:00:00Z"
author = "John Doe"
text = ""
seen = false
---

This is the content."#;
        let path = Path::new("test.md");

        let result = ld.parse_feed_entry(content, path).unwrap();

        assert_eq!(result.filepath, PathBuf::from("test.md"));
        assert_eq!(result.text, "This is the content.");
        assert_eq!(result.title, "Test Entry");
    }

    #[test]
    fn test_parse_feed_entry_invalid_format() {
        let (ld, _temp) = LibraryData::new_for_test();
        let content = "Invalid content without delimiters";
        let path = Path::new("test.md");

        let result = ld.parse_feed_entry(content, path);
        assert!(result.is_err());
    }
}
