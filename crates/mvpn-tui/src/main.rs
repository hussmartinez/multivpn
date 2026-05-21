mod app;
mod client;
mod ui;

use anyhow::Result;
use app::App;

fn main() -> Result<()> {
    let mut app = App::new();
    app.run()
}
