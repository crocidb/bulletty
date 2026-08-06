use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Debug, Serialize, Deserialize)]
pub struct FeedItem {
    pub title: String,
    pub description: String,
    pub url: String,
    pub feed_url: String,
    pub author: String,
    pub slug: String,
    pub lastupdated: DateTime<Utc>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,

    #[serde(skip_serializing, skip_deserializing)]
    pub category: String,
}
