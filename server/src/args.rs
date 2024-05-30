use clap::Parser;

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
}