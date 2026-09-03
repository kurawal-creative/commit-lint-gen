pub enum AppState {
    Menu,
    Editing,
    Manual,
    Loading(String),
    Quit,
}

pub struct App {
    pub commit_msg: String,
    pub edit_input: String,
    pub edit_cursor: usize,
    pub model: String,
    pub base_url: String,
    pub api_key: String,
    pub is_indo: bool,
    pub selected_option: usize,
    pub state: AppState,
    pub diff: String,
    pub pending: Option<std::sync::mpsc::Receiver<Result<String, String>>>,
    pub rt: tokio::runtime::Runtime,
    pub ticker: std::time::Instant,
}
