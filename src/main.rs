use anyhow::Result;
use clap::{Parser, ValueEnum};
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use gitgraph::git::LogOrder;
use std::io;
use std::panic;

#[derive(Debug, Clone, ValueEnum)]
enum Order {
    Chrono,
    Topo,
}

#[derive(Debug, Clone, ValueEnum)]
enum GraphWidth {
    Auto,
    Double,
    Single,
}

#[derive(Debug, Clone, ValueEnum)]
enum GraphStyle {
    Rounded,
    Angular,
}

#[derive(Debug, Clone, ValueEnum)]
enum Protocol {
    Auto,
    Iterm,
    Kitty,
}

#[derive(Parser, Debug)]
#[command(
    name = "gg",
    version = "0.1.0",
    about = "A TUI git commit explorer combining git log graph with file tree diff viewing"
)]
struct Cli {
    /// Git log ordering (chrono or topo)
    #[arg(long, value_enum, default_value_t = Order::Chrono)]
    order: Order,

    /// Graph column width
    #[arg(long, value_enum, default_value_t = GraphWidth::Auto)]
    graph_width: GraphWidth,

    /// Graph rendering style
    #[arg(long, value_enum, default_value_t = GraphStyle::Rounded)]
    graph_style: GraphStyle,

    /// Terminal image protocol
    #[arg(long, value_enum, default_value_t = Protocol::Auto)]
    protocol: Protocol,
}

fn setup_terminal() -> Result<ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = ratatui::backend::CrosstermBackend::new(stdout);
    let terminal = ratatui::Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<io::Stdout>>,
) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn main() -> Result<()> {
    let args = Cli::parse();

    let order = match args.order {
        Order::Chrono => LogOrder::Chronological,
        Order::Topo => LogOrder::Topological,
    };

    // Install panic hook to restore terminal before printing panic info
    let original_hook = panic::take_hook();
    panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    let mut terminal = setup_terminal()?;
    let result = gitgraph::run(&mut terminal, order);
    restore_terminal(&mut terminal)?;
    result
}
