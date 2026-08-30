use web::SiteConfig;

fn main() {
    let addr = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());

    let local = web::spawn(
        SiteConfig::new(addr, "BadOmen")
            .public("public")
            .files("files")
            .manifest("data/downloads.json"),
    )
    .expect("unable to start the preview server");

    println!("preview on http://{local}");

    loop {
        std::thread::park();
    }
}
