use std::{
    fs::File,
    io::{Read, Write},
};

fn main() {
    linker_be_nice();
    // make sure linkall.x is the last linker script (otherwise might cause problems with flip-link)
    println!("cargo:rustc-link-arg=-Tlinkall.x");

    let mut minify_config = minify_html::Cfg::default();
    minify_config.enable_possibly_noncompliant();
    minify_config.minify_css = true;

    let locked_html = "./src/locked.html";
    let unlocked_html = "./src/unlocked.html";

    minify_html_file(locked_html, "./src/locked.min.html", &minify_config);
    minify_html_file(unlocked_html, "./src/unlocked.min.html", &minify_config);
}

fn minify_html_file(file_path: &str, save_as: &str, minify_config: &minify_html::Cfg) {
    let mut file = File::open(file_path).expect(
        format!(
            "Minify ERROR: Could not open file {} for minification.",
            file_path
        )
        .as_str(),
    );
    let mut contents = String::new();
    file.read_to_string(&mut contents).expect(
        format!(
            "Minify ERROR: Could not read file {} for minification.",
            file_path
        )
        .as_str(),
    );

    let minified = minify_html::minify(contents.as_bytes(), minify_config);

    let mut save_file = File::create(save_as).expect(
        format!(
            "Minify ERROR: Could not create file {} to save minified html.",
            save_as
        )
        .as_str(),
    );
    save_file.write_all(&minified).expect(
        format!(
            "Minify ERROR: Could not write to minified html file {}",
            save_as
        )
        .as_str(),
    );
}

fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        let kind = &args[1];
        let what = &args[2];

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                "_defmt_timestamp" => {
                    eprintln!();
                    eprintln!("💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`");
                    eprintln!();
                }
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                "esp_wifi_preempt_enable"
                | "esp_wifi_preempt_yield_task"
                | "esp_wifi_preempt_task_create" => {
                    eprintln!();
                    eprintln!("💡 `esp-wifi` has no scheduler enabled. Make sure you have the `builtin-scheduler` feature enabled, or that you provide an external scheduler.");
                    eprintln!();
                }
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!("💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests");
                    eprintln!();
                }
                _ => (),
            },
            // we don't have anything helpful for "missing-lib" yet
            _ => {
                std::process::exit(1);
            }
        }

        std::process::exit(0);
    }

    println!(
        "cargo:rustc-link-arg=--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
