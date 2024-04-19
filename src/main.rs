mod document;

use dashmap::DashMap;
use env_logger::Env;
use log::{debug, error, info};
use lsp_server::{Connection, Message, ProtocolError, Request, Response};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, Hover, HoverContents, HoverParams,
    HoverProviderCapability, MarkedString, ServerCapabilities, TextDocumentSyncCapability,
    TextDocumentSyncKind,
};
use serde::Deserialize;
use thiserror::Error;

use document::Document;

#[derive(Debug, Error)]
enum LspError {
    #[error("Failed to serialize/deserialize JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Channel send error: {0}")]
    ChannelSend(#[from] crossbeam_channel::SendError<Message>),
}

type Result<T> = anyhow::Result<T>;

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    info!("Starting fluxion-lsp");

    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL,
        )),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        ..Default::default()
    };

    let initialization_params =
        connection.initialize(serde_json::to_value(server_capabilities)?)?;

    let backend = Backend::new();
    main_loop(&connection, initialization_params, backend)?;

    io_threads.join()?;
    info!("Shutting down server");
    Ok(())
}

struct Backend {
    documents: DashMap<String, Document>,
}

impl Backend {
    fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let document = Document::new(uri.clone(), params.text_document.text);
        if document.symbol_table.is_empty() {
            info!("Document has no symbols");
        }
        for symbol in document.symbol_table.iter() {
            info!("Symbol: {:?}", symbol);
        }
        self.documents.insert(uri.to_string(), document);
        info!("Opened document: {}", uri);
    }

    fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(mut document) = self
            .documents
            .get_mut(&params.text_document.uri.to_string())
        {
            if let Err(e) = document.update(&params.content_changes) {
                error!("Failed to apply changes: {:?}", e);
            } else {
                document.line_number_map =
                    Document::compute_line_number_map(&document.content.to_string());
                info!("Updated document: {}", params.text_document.uri);
            }
        }
    }

    fn hover(&self, params: HoverParams) -> Option<Hover> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let position = params.text_document_position_params.position;

        self.documents.get(&uri).map(|document| {
            let offset = document::to_rope_position(&document.content, position);
            let line = document.get_line_number(offset).unwrap_or(0) as usize;
            let character = offset - document.line_number_map[line];

            let mut symbol_info = None;
            for symbol in document.symbol_table.iter() {
                let start_line = symbol.location.range.start.line as usize;
                let start_character = symbol.location.range.start.character as usize;
                let end_line = symbol.location.range.end.line as usize;
                let end_character = symbol.location.range.end.character as usize;

                if line >= start_line
                    && line <= end_line
                    && (line == start_line && character >= start_character
                        || line == end_line && character <= end_character
                        || line > start_line && line < end_line)
                {
                    symbol_info = Some(symbol);
                    break;
                }
            }

            let hover_contents = if let Some(symbol) = symbol_info {
                format!(
                    "Symbol: `{}`\nKind: {:?}\nLocation: {:?}",
                    symbol.name, symbol.kind, symbol.location
                )
            } else {
                format!(
                    "Character: `{}`\nOffset: {}\nLine: {}\nCharacter: {}\nURI: {}",
                    document.content.char(offset),
                    offset,
                    line,
                    character,
                    uri
                )
            };

            Hover {
                contents: HoverContents::Scalar(MarkedString::String(hover_contents)),
                range: None,
            }
        })
    }
}

fn main_loop(connection: &Connection, _params: serde_json::Value, backend: Backend) -> Result<()> {
    for msg in &connection.receiver {
        match msg {
            Message::Request(req) => {
                if let Some(resp) = handle_request(&backend, req)? {
                    connection.sender.send(Message::Response(resp))?;
                }
            }
            Message::Response(resp) => {
                debug!("Got response: {:?}", resp);
            }
            Message::Notification(not) => {
                handle_notification(&backend, not)?;
            }
        }
    }
    Ok(())
}

fn handle_request(backend: &Backend, req: Request) -> Result<Option<Response>> {
    let id = req.id.clone();
    match req.method.as_str() {
        "textDocument/hover" => {
            let params = from_value::<HoverParams>(req.params)?;
            let hover = backend.hover(params);
            Ok(Some(Response {
                id,
                result: hover.map(|h| serde_json::to_value(h).unwrap()),
                error: None,
            }))
        }
        _ => Ok(None),
    }
}

fn handle_notification(backend: &Backend, not: lsp_server::Notification) -> Result<()> {
    match not.method.as_str() {
        "textDocument/didOpen" => {
            let params = from_value::<DidOpenTextDocumentParams>(not.params)?;
            backend.did_open(params);
        }
        "textDocument/didChange" => {
            let params = from_value::<DidChangeTextDocumentParams>(not.params)?;
            backend.did_change(params);
        }
        _ => {}
    }
    Ok(())
}

fn from_value<T: for<'a> Deserialize<'a>>(value: serde_json::Value) -> Result<T> {
    serde_json::from_value(value).map_err(Into::into)
}
