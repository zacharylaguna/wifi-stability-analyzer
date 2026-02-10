use anyhow::Result;
use tracing::info;
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
    dpi::LogicalSize,
};
use wry::WebViewBuilder;
use rfd::MessageDialog;

pub fn launch_gui(port: u16) -> Result<()> {
    info!("Launching GUI window on port {}", port);

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("WiFi Stability Tracker")
        .with_inner_size(LogicalSize::new(1400, 900))
        .with_resizable(true)
        .build(&event_loop)?;

    let url = format!("http://localhost:{}", port);
    
    let _webview = WebViewBuilder::new(&window)
        .with_url(&url)
        .build()?;

    info!("GUI window created, loading dashboard from {}", url);

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                // Show confirmation dialog
                let result = MessageDialog::new()
                    .set_title("Exit WiFi Stability Tracker")
                    .set_description("Are you sure you want to stop monitoring and exit?\n\nAll background monitoring will be stopped.")
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show();

                if result == rfd::MessageDialogResult::Yes {
                    info!("User confirmed exit - shutting down");
                    *control_flow = ControlFlow::Exit;
                    
                    // Force process exit to stop all background threads
                    std::process::exit(0);
                } else {
                    info!("User canceled exit");
                }
            }
            _ => {}
        }
    });
}
