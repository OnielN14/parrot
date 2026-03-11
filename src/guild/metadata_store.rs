use std::collections::HashMap;

use serenity::prelude::TypeMapKey;
use songbird::input::AuxMetadata;

#[derive(Debug, Clone, Default)]
pub struct MetadataStore {
    store: HashMap<String, AuxMetadata>,
}

impl MetadataStore {
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
        }
    }

    pub fn retrieve_metadata(&self, song_reference: &str) -> Option<&AuxMetadata> {
        self.store.get(song_reference)
    }

    pub fn insert_metadata(&mut self, song_reference: &str, metadata: AuxMetadata) {
        self.store.insert(song_reference.to_string(), metadata);
    }

    pub fn remove_metadata(&mut self, song_reference: &str) {
        self.store.remove(song_reference);
    }
}

impl TypeMapKey for MetadataStore {
    type Value = MetadataStore;
}
