//! Provides the `Document` struct and related functionality for handling text documents in the LSP server.

use log::debug;
use lsp_types::{
    Location, Position, Range, SymbolInformation, SymbolKind, TextDocumentContentChangeEvent, Url,
};
use ropey::Rope;
use ruff_python_ast::{
    Expr, ExprName, ModModule, Stmt, StmtAnnAssign, StmtAssign, StmtClassDef, StmtFor,
    StmtFunctionDef, StmtImport, StmtImportFrom,
};
use ruff_python_parser::parse_program;
use ruff_text_size::TextRange;
use std::collections::HashMap;
use thiserror::Error;

/// Represents a text document and its associated data.
///
/// The `Document` struct holds the following information:
/// - The URI of the document.
/// - The content of the document as a `Rope` data structure.
/// - The parsed AST of the document, if available.
/// - A mapping of character offsets to line numbers.
/// - The symbol table of the document.
pub struct Document {
    pub uri: Url,
    pub content: Rope,
    pub ast: Option<ModModule>,
    pub line_number_map: Vec<usize>,
    pub symbol_table: SymbolTable,
}

#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("Failed to parse the document: {0}")]
    ParseError(#[from] ruff_python_parser::ParseError),
}

impl Document {
    /// Creates a new `Document` instance with the given URI and content.
    pub fn new(uri: Url, content: String) -> Self {
        let rope = Rope::from_str(&content);
        let line_number_map = Self::compute_line_number_map(&content);
        let ast = parse_rope_to_ast(&rope).ok();
        debug!("Parsed AST: {:#?}", ast);

        let mut document = Document {
            uri,
            content: rope,
            ast,
            line_number_map,
            symbol_table: SymbolTable::new(),
        };

        document.symbol_table = document.compute_symbol_table();

        document
    }

    /// Updates the document with the given changes and recomputes the AST and symbol table.
    pub fn update(
        &mut self,
        changes: &[TextDocumentContentChangeEvent],
    ) -> Result<(), DocumentError> {
        apply_changes(&mut self.content, changes)?;
        self.line_number_map = Self::compute_line_number_map(&self.content.to_string());
        self.ast = parse_rope_to_ast(&self.content).ok();
        self.symbol_table = self.compute_symbol_table();
        Ok(())
    }

    /// Computes the line number mapping for the given document content.
    pub fn compute_line_number_map(content: &str) -> Vec<usize> {
        let mut line_number_map = Vec::new();
        let mut char_count = 0;

        for line in content.lines() {
            line_number_map.push(char_count);
            char_count += line.len() + 1; // +1 for the newline character
        }

        line_number_map
    }

    /// Retrieves the line number for the given character offset in the document.
    pub fn get_line_number(&self, offset: usize) -> Option<u32> {
        self.line_number_map.binary_search(&offset).map_or_else(
            |index| index.checked_sub(1).map(|i| i as u32),
            |index| Some(index as u32),
        )
    }

    /// Computes the symbol table for the document based on its AST.
    fn compute_symbol_table(&self) -> SymbolTable {
        let mut symbol_table = SymbolTable::new();

        if let Some(ast) = &self.ast {
            for stmt in &ast.body {
                if let Some(symbol_info) = self.create_symbol_info(stmt) {
                    symbol_table.insert(symbol_info);
                }
            }
        }

        symbol_table
    }

    /// Creates a `SymbolInformation` instance for the given AST statement.
    #[allow(deprecated)]
    fn create_symbol_info(&self, stmt: &Stmt) -> Option<SymbolInformation> {
        match stmt {
            Stmt::FunctionDef(StmtFunctionDef { name, range, .. }) => Some(SymbolInformation {
                name: name.to_string(),
                kind: SymbolKind::FUNCTION,
                tags: None,
                deprecated: None,
                location: self.create_location(range),
                container_name: None,
            }),
            Stmt::ClassDef(StmtClassDef { name, range, .. }) => Some(SymbolInformation {
                name: name.to_string(),
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                location: self.create_location(range),
                container_name: None,
            }),
            Stmt::Assign(StmtAssign { targets, range, .. }) => {
                if let Some(Expr::Name(ExprName { id, .. })) = targets.first() {
                    Some(SymbolInformation {
                        name: id.to_string(),
                        kind: SymbolKind::VARIABLE,
                        tags: None,
                        deprecated: None,
                        location: self.create_location(range),
                        container_name: None,
                    })
                } else {
                    None
                }
            }
            Stmt::AnnAssign(StmtAnnAssign { target, range, .. }) => {
                if let Expr::Name(ExprName { id, .. }) = target.as_ref() {
                    Some(SymbolInformation {
                        name: id.to_string(),
                        kind: SymbolKind::VARIABLE,
                        tags: None,
                        deprecated: None,
                        location: self.create_location(range),
                        container_name: None,
                    })
                } else {
                    None
                }
            }
            Stmt::For(StmtFor { target, range, .. }) => {
                if let Expr::Name(ExprName { id, .. }) = target.as_ref() {
                    Some(SymbolInformation {
                        name: id.to_string(),
                        kind: SymbolKind::VARIABLE,
                        tags: None,
                        deprecated: None,
                        location: self.create_location(range),
                        container_name: None,
                    })
                } else {
                    None
                }
            }
            Stmt::Import(StmtImport { names, range, .. }) => {
                names.first().map(|name| SymbolInformation {
                    name: name.name.to_string(),
                    kind: SymbolKind::MODULE,
                    tags: None,
                    deprecated: None,
                    location: self.create_location(range),
                    container_name: None,
                })
            }
            Stmt::ImportFrom(StmtImportFrom { module, range, .. }) => {
                let name = module
                    .as_ref()
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let location = self.create_location(range);
                Some(SymbolInformation {
                    name,
                    kind: SymbolKind::MODULE,
                    tags: None,
                    deprecated: None,
                    location,
                    container_name: None,
                })
            }
            _ => None,
        }
    }

    /// Creates a `Location` instance for the given text range in the document.
    fn create_location(&self, range: &TextRange) -> Location {
        let start_offset = range.start().to_usize();
        let end_offset = range.end().to_usize();

        let start_line = self
            .line_number_map
            .binary_search(&start_offset)
            .unwrap_or_else(|index| index.saturating_sub(1));

        let end_line = self
            .line_number_map
            .binary_search(&end_offset)
            .unwrap_or_else(|index| index.saturating_sub(1));

        let start_character = start_offset - self.line_number_map[start_line];
        let end_character = end_offset - self.line_number_map[end_line];

        Location {
            uri: self.uri.clone(),
            range: Range {
                start: Position {
                    line: start_line as u32,
                    character: start_character as u32,
                },
                end: Position {
                    line: end_line as u32,
                    character: end_character as u32,
                },
            },
        }
    }
}

/// Represents a symbol table, which maps symbol names to their corresponding `SymbolInformation`.
#[derive(Debug, Clone)]
pub struct SymbolTable {
    symbols: HashMap<String, SymbolInformation>,
}

#[allow(dead_code)]
impl SymbolTable {
    /// Creates a new empty `SymbolTable`.
    pub fn new() -> Self {
        SymbolTable {
            symbols: HashMap::new(),
        }
    }

    /// Inserts a `SymbolInformation` into the symbol table.
    pub fn insert(&mut self, symbol: SymbolInformation) {
        self.symbols.insert(symbol.name.clone(), symbol);
    }

    /// Retrieves a `SymbolInformation` from the symbol table by its name.
    pub fn get(&self, name: &str) -> Option<&SymbolInformation> {
        self.symbols.get(name)
    }

    /// Checks if the symbol table contains a symbol with the given name.
    pub fn contains(&self, name: &str) -> bool {
        self.symbols.contains_key(name)
    }

    /// Returns an iterator over the `SymbolInformation` values in the symbol table.
    pub fn iter(&self) -> impl Iterator<Item = &SymbolInformation> {
        self.symbols.values()
    }

    /// Returns the number of symbols in the symbol table.
    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    /// Checks if the symbol table is empty.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }
}

/// Converts a `Position` in a text document to the corresponding byte offset in a `Rope`.
pub fn to_rope_position(document: &Rope, position: Position) -> usize {
    document.line_to_char(position.line as usize) + position.character as usize
}

/// Parses the content of a `Rope` into an AST.
pub fn parse_rope_to_ast(rope: &Rope) -> Result<ModModule, DocumentError> {
    let code = rope.to_string();
    parse_program(&code).map_err(DocumentError::from)
}

/// Applies a set of changes to a `Rope` document.
pub fn apply_changes(
    document: &mut Rope,
    changes: &[TextDocumentContentChangeEvent],
) -> Result<(), DocumentError> {
    for change in changes {
        if let Some(range) = change.range {
            let start = to_rope_position(document, range.start);
            let end = to_rope_position(document, range.end);
            document.remove(start..end);
            document.insert(start, &change.text);
        } else {
            *document = Rope::from_str(&change.text);
        }
    }
    Ok(())
}
