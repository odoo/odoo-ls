use clap::{Parser, ValueEnum};

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    //Do not run the server, but only extract diagnostics from the codebase, then stop.
    #[arg(short, long)]
    pub parse: bool,

    //addon paths you want to parse (parse mode required)
    #[arg(short, long)]
    pub addons: Option<Vec<String>>,

    //community path (parse mode required)
    #[arg(short, long)]
    pub community_path: Option<String>,

    //Tracked folders. Diagnostics will only be raised if they are in a file inside one of these directory
    //by default populated with all odoo directories + addon paths (parse mode required)
    #[arg(short, long)]
    pub tracked_folders: Option<Vec<String>>,

    //python path to use (parse mode required)
    #[arg(long)]
    pub python: Option<String>,

    //output path. Default to "output.json"
    #[arg(short, long)]
    pub output: Option<String>,

    #[arg(short, long)]
    //additional stubs directories. Be careful that each stub must be in a directory with its own name.
    pub stubs: Option<Vec<String>>,

    //Remove Typeshed stubs. Useful if you want to provide your own version of stubs. It does not remove stdlib stubs however (they are required), only stubs of external packages
    #[arg(long)]
    pub no_typeshed: bool,

    //give an alternative path to stdlib stubs. 
    #[arg(long)]
    pub stdlib: Option<String>,

    //Provide a pid (unix only) that the server will listen and kill itself if the process stop.
    #[arg(long)]
    pub clientProcessId: Option<u32>,

    #[arg(long)]
    pub use_tcp: bool,

    #[arg(value_enum, long, default_value="trace")]
    pub log_level: LogLevel,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum LogLevel {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR,
}