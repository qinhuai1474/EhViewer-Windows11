//! Phase 2 acceptance smoke test. Fetches homepage + popular and parses.

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(run());
}

async fn run() {
    use ehviewer_windows11_lib::client::{client, engine, parser};
    use ehviewer_windows11_lib::client::list_url::{ListQuery, MODE_NORMAL, MODE_WHATS_HOT};

    for (name, mode) in [("homepage", MODE_NORMAL), ("popular", MODE_WHATS_HOT)] {
        let url = ListQuery { mode, site: 0, ..Default::default() }.build();
        // Ensure uconfig cookie is applied (engine does this; replicate for raw fetch).
        engine::apply_uconfig();
        match client::get_text(&url, None).await {
            Ok(body) => {
                println!("\n[{name}] len={}", body.len());
                match parser::gallery_list::parse(&body) {
                    Ok(result) => {
                        println!("[{name}] parsed items={} pages={}", result.items.len(), result.nav.pages);
                        for it in result.items.iter().take(3) {
                            println!("    gid={} cat={} pages={} rating={} title={}", it.gid, it.category, it.pages, it.rating, it.title);
                        }
                    }
                    Err(e) => println!("[{name}] parse error: {e}"),
                }
                let head: String = body.chars().take(900).collect();
                println!("[{name}] BODY HEAD:\n{head}\n---END---");
            }
            Err(e) => println!("[{name}] fetch error: {e}"),
        }
    }
}
