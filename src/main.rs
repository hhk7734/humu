mod app;

use anyhow::Result;

fn main() -> Result<()> {
    let mut app = app::App::new()?;
    app.run()
}
