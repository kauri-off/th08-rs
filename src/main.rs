fn main() {
    env_logger::init();
    let dir = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("TH08_DIR").ok())
        .expect("usage: th08 <game dir>");
    th08::app::run(th08::assets::Source::native(dir.into()));
}
