use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(version, about, long_about = None)]
#[allow(non_snake_case)]
pub struct Cli {
    /// Do not run the server, but only extract diagnostics from the codebase, then stop.
    #[arg(short, long)]
    pub parse: bool,

    /// Addon paths you want to parse (parse mode required)
    #[arg(short, long)]
    pub addons: Option<Vec<String>>,

    /// Community path (parse mode required)
    #[arg(short, long)]
    pub community_path: Option<String>,

    /// Tracked folders (parse mode required). Diagnostics will only be raised if they are in a file inside one of these directories.
    /// By default populated with all Odoo directories + addon paths.
    /// These act as workspace folders when processing config files in parse mode.
    #[arg(short, long)]
    pub tracked_folders: Option<Vec<String>>,

    /// Python path to use (parse mode required)
    #[arg(long)]
    pub python: Option<String>,

    /// Output path. Default to "output.json"
    #[arg(short, long)]
    pub output: Option<String>,

    /// Additional stubs directories. Be careful that each stub must be in a directory with its own name.
    #[arg(short, long)]
    pub stubs: Option<Vec<String>>,

    /// Remove Typeshed stubs. Useful if you want to provide your own version of stubs. It does not remove stdlib stubs however (they are required), only stubs of external packages
    #[arg(long)]
    pub no_typeshed_stubs: bool,

    /// Give an alternative path to stdlib stubs.
    #[arg(long)]
    pub stdlib: Option<String>,

    /// Provide a pid (unix only) that the server will listen and kill itself if the process stop.
    #[arg(long)]
    pub clientProcessId: Option<u32>,

    /// Use TCP connection instead of stdio, for debug mode. On localhost at port 2087.
    #[arg(long)]
    pub use_tcp: bool,

    #[arg(value_enum, long, default_value="trace")]
    pub log_level: LogLevel,

    /// Provide a path to the directory that will be used for logs
    #[arg(long)]
    pub logs_directory: Option<String>,

    /// Default config file path
    #[arg(long)]
    pub config_path: Option<String>,

    /// Selected config profile to be used
    #[arg(long)]
    pub selected_config: Option<String>,

}

#[derive(ValueEnum, Clone, Debug)]
pub enum LogLevel {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
}