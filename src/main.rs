use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, BufRead, Write},
    os::unix::prelude::OsStringExt,
    path::{self, PathBuf},
    process::{self, Command, Stdio},
};

use ansi_term::Colour;
use regex::{Captures, Regex};

// 🚧 I should return something, and at least allow for returning errors of some
// of my subprocesses and function calls and whatnot.
fn main() -> io::Result<()> {
    let out_path: PathBuf = ["target", "coverage"].iter().collect();
    let deps_path: PathBuf = ["target", "debug", "deps"].iter().collect();
    let current_dir = env::current_dir().unwrap();
    let (root, deps) = {
        let pkg_dir = find_package_dir(&None).unwrap();
        (pkg_dir.join(&out_path), pkg_dir.join(&deps_path))
    };

    match Command::new("grcov").arg("-h").output() {
        Ok(_) => {}
        Err(_) => {
            eprintln!(
                "🚧 {0} is not installed Please install {0} to continue. See {1}.",
                Colour::Yellow.italic().paint("grcov"),
                Colour::Blue
                    .italic()
                    .paint("https://github.com/mozilla/grcov")
            );
            std::process::exit(1);
        }
    };

    // Remove all existing profraw files
    if root.exists() {
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(extension) = path.extension() {
                if extension == "profraw" {
                    fs::remove_file(&path)?;
                }
            }
        }
    }

    let mut args: std::collections::VecDeque<String> = env::args().skip(1).collect();
    /* cargo invocation */
    if args.front().is_some_and(|arg| arg == "cover") {
        args.pop_front();
    }

    let child = Command::new("cargo")
        .arg("test")
        .args(&args)
        .env("CARGO_INCREMENTAL", "0")
        .env("RUSTFLAGS", "-Cinstrument-coverage")
        .env("RUSTDOCFLAGS", "-Cinstrument-coverage")
        .env(
            "LLVM_PROFILE_FILE",
            out_path.join("cargo-test-%p-%m.profraw").to_str().unwrap(),
        )
        .spawn()?;
    let _ = child.wait_with_output().expect("failed to wait on child");

    let lcov_file = out_path.join(["output", "lcov.info"].into_iter().collect::<PathBuf>());
    let _ = fs::remove_file(&lcov_file);
    let args = [
        ".",
        "--binary-path",
        deps.to_str().unwrap_or_default(),
        "-s",
        ".",
        "--branch",
        "--ignore-not-existing",
        "--ignore",
        "../*",
        "--ignore",
        "/*",
        "--excl-start",
        "^mod test",
        "--excl-stop",
        "^}",
    ];

    eprintln!(
        "{} markdown coverage report",
        Colour::Green.bold().paint(format!("{:>12}", "Generating"))
    );
    let mut child = Command::new("grcov")
        .args(args)
        .arg("-t")
        .arg("markdown")
        .stdout(Stdio::piped())
        .current_dir(&current_dir)
        .spawn()?;
    let out = child.stdout.take().expect("failed to parse grcov output");
    {
        let pct_regex = Regex::new(r"(\d+(:?\.\d+)?)%").unwrap();
        let mut lock = io::stdout().lock();
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            let line = pct_regex.replace_all(&line, |cap: &Captures| {
                let num_str = &cap[1];
                match num_str.parse::<f32>() {
                    Ok(num) if num > 90f32 => Colour::Green.bold().paint(&cap[0]).to_string(),
                    Ok(num) if num > 75f32 => Colour::Yellow.bold().paint(&cap[0]).to_string(),
                    Ok(_) => Colour::Red.bold().paint(&cap[0]).to_string(),
                    _ => cap[0].to_string(),
                }
            });
            lock.write_all(line.as_bytes())?;
            lock.write_all(b"\n")?;
        }
    }
    let _ = child.wait_with_output()?;

    let html_out_dir = out_path.join("output");
    let html_out_dir = path::absolute(&html_out_dir).unwrap_or(html_out_dir);
    eprintln!(
        "{} html coverage report ({})",
        Colour::Green.bold().paint(format!("{:>12}", "Generating")),
        html_out_dir.join(["html", "index.html"].into_iter().collect::<PathBuf>()).to_str().unwrap()
    );
    fs::create_dir_all(&html_out_dir)?;
    let child = Command::new("grcov")
        .args(args)
        .arg("-t")
        .arg("html")
        .arg("-o")
        .arg(html_out_dir)
        .current_dir(&current_dir)
        .spawn()?;
    let _ = child.wait_with_output()?;

    eprintln!(
        "{} lcov coverage report ({})",
        Colour::Green.bold().paint(format!("{:>12}", "Generating")),
        lcov_file.to_str().unwrap()
    );
    /* finish with lcov since its report would be parsed by grcov */
    let child = Command::new("grcov")
        .args(args)
        .arg("-t")
        .arg("lcov")
        .arg("-o")
        .arg(lcov_file)
        .current_dir(&current_dir)
        .spawn()?;
    let _ = child.wait_with_output()?;

    Ok(())
}

fn find_package_dir(start_dir: &Option<PathBuf>) -> Result<PathBuf, std::io::Error> {
    if let Some(dir) = start_dir {
        std::env::set_current_dir(dir)?;
    }

    // Figure out where Cargo.toml is located.
    //
    let output = process::Command::new("cargo")
        .arg("locate-project")
        .arg("--message-format")
        .arg("plain")
        .output()?;
    // .context(
    //     "😱 Tried running `cargo locate-project to no avail. \
    //         Maybe you need to add cargo to you path?",
    // )?;

    // anyhow::ensure!(
    //     output.status.success(),
    //     format!(
    //         "😱 Unable to find package in directory: {:?}.",
    //         std::env::current_dir()?
    //     )
    // );

    let mut stdout = output.stdout;

    // I don't know if it's kosher, but this does nicely to get rid of
    // that newline character.
    stdout.pop();
    let os_string = OsString::from_vec(stdout);
    let mut package_root = PathBuf::from(os_string);
    // Get rid of Cargo.toml
    package_root.pop();

    // debug!("Found root 🦀 at {:?}!", package_root);

    Ok(package_root)
}
