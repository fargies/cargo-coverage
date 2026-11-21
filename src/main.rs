use std::{
    env,
    ffi::OsStr,
    fs,
    io::{self, BufRead, Error, Write},
    os::unix::ffi::OsStrExt,
    path::{self, PathBuf},
    process::{self, Command, Stdio},
};

use ansi_term::Colour;
use regex::{Captures, Regex};

// 🚧 I should return something, and at least allow for returning errors of some
// of my subprocesses and function calls and whatnot.
fn main() -> io::Result<()> {
    let workspace_dir = find_package_dir()?;
    let target_dir = workspace_dir.join("target");
    let coverage_dir = target_dir.join(PathBuf::from("coverage"));
    let deps = if let path = target_dir.join(PathBuf::from_iter(["debug", "deps"]))
        && path.exists()
    {
        path
    } else {
        /* cargo >= 1.100.0 doesn't have "deps" dir anymore */
        target_dir.join(PathBuf::from_iter(["debug", "build"]))
    };
    let current_dir = env::current_dir().unwrap();

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
    if coverage_dir.exists() {
        for entry in fs::read_dir(&coverage_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(extension) = path.extension()
                && extension == "profraw"
            {
                fs::remove_file(&path)?;
            }
        }
    }

    let mut args: std::collections::VecDeque<String> = env::args().skip(1).collect();
    /* cargo invocation */
    if args.front().is_some_and(|arg| arg == "cover") {
        args.pop_front();
    }

    if !Command::new("cargo")
        .arg("test")
        .args(&args)
        .envs([
            ("CARGO_COVER", "1"),
            ("CARGO_INCREMENTAL", "0"),
            ("RUSTFLAGS", "-Cinstrument-coverage"),
            ("RUSTDOCFLAGS", "-Cinstrument-coverage"),
            (
                "LLVM_PROFILE_FILE",
                coverage_dir
                    .join("cargo-test-%p-%m.profraw")
                    .to_str()
                    .unwrap(),
            ),
        ])
        .status()?
        .success()
    {
        // may not be an issue if some tests are failing
        eprintln!("cargo test failed");
    }

    let lcov_file = coverage_dir.join(["output", "lcov.info"].into_iter().collect::<PathBuf>());
    let _ = fs::remove_file(&lcov_file);
    let mut args = vec![
        ".",
        "--binary-path",
        deps.to_str().unwrap_or_default(),
        "-s",
        ".",
        "--prefix-dir",
        workspace_dir.to_str().unwrap_or_default(),
        "--branch",
        "--ignore-not-existing",
        "--ignore",
        "../*",
        "--ignore",
        "/*",
        "--ignore",
        "**/tests/*",
        "--excl-start",
        "^mod test",
        "--excl-stop",
        "^}",
    ];
    let keep_path = match current_dir.strip_prefix(&workspace_dir) {
        Ok(path) if !path.is_empty() => format!("{}/*", path.to_str().unwrap_or_default()),
        _ => String::default(),
    };
    if !keep_path.is_empty() {
        args.extend(["--keep-only", keep_path.as_str()]);
    }

    eprintln!(
        "{} markdown coverage report",
        Colour::Green.bold().paint(format!("{:>12}", "Generating"))
    );
    let mut child = Command::new("grcov")
        .args(&args)
        .arg("-t")
        .arg("markdown")
        .stdout(Stdio::piped())
        .current_dir(&workspace_dir)
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

    let html_out_dir = coverage_dir.join("output");
    let html_out_dir = path::absolute(&html_out_dir).unwrap_or(html_out_dir);
    eprintln!(
        "{} html coverage report ({})",
        Colour::Green.bold().paint(format!("{:>12}", "Generating")),
        html_out_dir
            .join(["html", "index.html"].into_iter().collect::<PathBuf>())
            .to_str()
            .unwrap()
    );
    fs::create_dir_all(&html_out_dir)?;
    let child = Command::new("grcov")
        .args(&args)
        .arg("-t")
        .arg("html")
        .arg("-o")
        .arg(html_out_dir)
        .current_dir(&workspace_dir)
        .spawn()?;
    let _ = child.wait_with_output()?;

    eprintln!(
        "{} lcov coverage report ({})",
        Colour::Green.bold().paint(format!("{:>12}", "Generating")),
        lcov_file.to_str().unwrap()
    );
    /* finish with lcov since its report would be parsed by grcov */
    let child = Command::new("grcov")
        .args(&args)
        .arg("-t")
        .arg("lcov")
        .arg("-o")
        .arg(lcov_file)
        .current_dir(&workspace_dir)
        .spawn()?;
    let _ = child.wait_with_output()?;

    Ok(())
}

fn find_package_dir() -> Result<PathBuf, Error> {
    // Figure out where Cargo.toml is located.
    //
    let output = process::Command::new("cargo")
        .args(["locate-project", "--message-format", "plain", "--workspace"])
        .output()?;

    let stdout = output.stdout.trim_ascii();
    let os_string = OsStr::from_bytes(stdout);
    let mut package_root = PathBuf::from(os_string);
    // Get rid of Cargo.toml
    package_root.pop();

    // debug!("Found root 🦀 at {:?}!", package_root);

    Ok(package_root)
}
