use std::{
    collections::HashMap,
    time::Instant,
    sync::Arc,
    panic,
};

use once_cell::sync::OnceCell;
use rayon::prelude::*;
use libeditor::LineIndex;
use libsyntax2::File;

use {
    FileId,
    module_map::{ModuleMap, ChangeKind},
    symbol_index::FileSymbols,
};

#[derive(Clone, Default, Debug)]
pub(crate) struct SourceRoot {
    file_map: HashMap<FileId, Arc<(FileData, OnceCell<FileSymbols>)>>,
    module_map: ModuleMap,
}

impl SourceRoot {
    pub fn update(&mut self, file_id: FileId, text: Option<String>) {
        let change_kind = if self.file_map.remove(&file_id).is_some() {
            if text.is_some() {
                ChangeKind::Update
            } else {
                ChangeKind::Delete
            }
        } else {
            ChangeKind::Insert
        };
        self.module_map.update_file(file_id, change_kind);
        self.file_map.remove(&file_id);
        if let Some(text) = text {
            let file_data = FileData::new(text);
            self.file_map.insert(file_id, Arc::new((file_data, Default::default())));
        }
    }
    pub fn module_map(&self) -> &ModuleMap {
        &self.module_map
    }
    pub fn lines(&self, file_id: FileId) -> &LineIndex {
        let data = self.data(file_id);
        data.lines.get_or_init(|| LineIndex::new(&data.text))
    }
    pub fn syntax(&self, file_id: FileId) -> &File {
        let data = self.data(file_id);
        let text = &data.text;
        let syntax = &data.syntax;
        match panic::catch_unwind(panic::AssertUnwindSafe(|| syntax.get_or_init(|| File::parse(text)))) {
            Ok(file) => file,
            Err(err) => {
                error!("Parser paniced on:\n------\n{}\n------\n", &data.text);
                panic::resume_unwind(err)
            }
        }
    }
    pub(crate) fn symbols(&self) -> Vec<&FileSymbols> {
        self.file_map
            .iter()
            .map(|(&file_id, data)| symbols(file_id, data))
            .collect()
    }
    pub fn reindex(&self) {
        let now = Instant::now();
        self.file_map
            .par_iter()
            .for_each(|(&file_id, data)| {
                symbols(file_id, data);
            });
        info!("parallel indexing took {:?}", now.elapsed());

    }
    fn data(&self, file_id: FileId) -> &FileData {
        match self.file_map.get(&file_id) {
            Some(data) => &data.0,
            None => panic!("unknown file: {:?}", file_id),
        }
    }
}

fn symbols(file_id: FileId, (data, symbols): &(FileData, OnceCell<FileSymbols>)) -> &FileSymbols {
    let syntax = data.syntax_transient();
    symbols.get_or_init(|| FileSymbols::new(file_id, &syntax))
}

#[derive(Debug)]
struct FileData {
    text: String,
    lines: OnceCell<LineIndex>,
    syntax: OnceCell<File>,
}

impl FileData {
    fn new(text: String) -> FileData {
        FileData {
            text,
            syntax: OnceCell::new(),
            lines: OnceCell::new(),
        }
    }
    fn syntax_transient(&self) -> File {
        self.syntax.get().map(|s| s.clone())
            .unwrap_or_else(|| File::parse(&self.text))
    }
}

// #[derive(Clone, Default, Debug)]
// pub(crate) struct ReadonlySourceRoot {
//     data: Arc<ReadonlySourceRoot>
// }

// #[derive(Clone, Default, Debug)]
// pub(crate) struct ReadonlySourceRootInner {
//     file_map: HashMap<FileId, FileData>,
//     module_map: ModuleMap,
// }