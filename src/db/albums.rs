//! Types for user tags and albums.

#![allow(dead_code)]

/// A user-defined tag
#[derive(Debug, Clone)]
pub struct UserTag {
    pub id: i64,
    pub name: String,
    pub color: String,
}

/// An album (collection of photos)
#[derive(Debug, Clone)]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub cover_photo_id: Option<i64>,
    pub is_smart: bool,
    pub filter_tags: Vec<i64>,
    pub photo_count: i64,
}
