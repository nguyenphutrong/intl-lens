use ropey::Rope;
use std::collections::HashMap;

pub struct DocumentStore {
    documents: HashMap<String, Document>,
}

pub struct Document {
    /// URI of the document - kept for debugging and future use
    #[allow(dead_code)]
    pub uri: String,
    pub content: Rope,
    pub version: i32,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    pub fn open(&mut self, uri: String, content: String, version: i32) {
        self.documents.insert(
            uri.clone(),
            Document {
                uri,
                content: Rope::from_str(&content),
                version,
            },
        );
    }

    pub fn update(&mut self, uri: &str, content: String, version: i32) {
        if let Some(doc) = self.documents.get_mut(uri) {
            doc.content = Rope::from_str(&content);
            doc.version = version;
        }
    }

    pub fn close(&mut self, uri: &str) {
        self.documents.remove(uri);
    }

    pub fn get(&self, uri: &str) -> Option<&Document> {
        self.documents.get(uri)
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}
