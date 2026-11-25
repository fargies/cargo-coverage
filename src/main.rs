/*
 ** Copyright (C) 2026 Sylvain Fargier
 **
 ** This software is provided 'as-is', without any express or implied
 ** warranty.  In no event will the authors be held liable for any damages
 ** arising from the use of this software.
 **
 ** Permission is granted to anyone to use this software for any purpose,
 ** including commercial applications, and to alter it and redistribute it
 ** freely, subject to the following restrictions:
 **
 ** 1. The origin of this software must not be misrepresented; you must not
 **    claim that you wrote the original software. If you use this software
 **    in a product, an acknowledgment in the product documentation would be
 **    appreciated but is not required.
 ** 2. Altered source versions must be plainly marked as such, and must not be
 **    misrepresented as being the original software.
 ** 3. This notice may not be removed or altered from any source distribution.
 **
 ** Author: Sylvain Fargier <fargier.sylvain@gmail.com>
 */

use crate::revarg::{RevParsedArgs, revarg_parse};
use colored::Colorize;
use regex::{Captures, Regex};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, BufRead, Error, Write},
    os::unix::ffi::OsStrExt,
    path::{self, PathBuf},
    process::{self, Command, Stdio},
};

mod revarg;

#[derive(Default, Debug)]
struct Args<'a> {
    cargo: Vec<&'a OsStr>,
    grcov: Vec<OsString>,
}

fn print_help() -> ! {
    println!("\n{}\n", "Cargo coverage utility".bold());

    println!(
        "This tool will run {} and {} with builtin flags and options.\n",
        "cargo-test".italic(),
        "grcov".italic()
    );

    println!(
        "{} {}\n",
        "Usage:".bold().underline(),
        "cargo cover [OPTIONS]".purple().italic()
    );

    println!("{}", "Grcov Options:".bold().underline());
    println!(
        "  Any option starting with {} will be passed-through to grcov, along with its eventual values.",
        "\"--grcov-\"".yellow().italic()
    );
    println!(
        "  To view available options, run: {}\n",
        "`grcov --help`".purple().italic()
    );
    println!(
        "  {} short options are not supported.\n",
        "Note:".bold().italic()
    );
    println!(
        "  {} {}\n",
        "Example:".bold().italic(),
        "`cargo cover --grcov-keep-only 'src/*'`".purple().italic()
    );

    println!("{}", "Cargo Options:".bold().underline());
    println!(
        "  Any option {} starting with {} will be passed-through to {}.",
        "not".bold(),
        "\"--grcov-\"".yellow().italic(),
        "`cargo test`".purple().italic()
    );
    println!(
        "  To view available options, run: {}\n",
        "`cargo test --help`".purple().italic()
    );
    std::process::exit(0)
}

fn parse_args<'a>(raw: &'a clap_lex::RawArgs, grcov: &RevParsedArgs) -> Args<'a> {
    let mut cursor = raw.cursor();
    let _bin = raw.next_os(&mut cursor);

    /* cargo invocation */
    if raw.peek_os(&cursor).is_some_and(|arg| arg == "cover") {
        raw.next_os(&mut cursor);
    }

    let mut ret = Args::default();
    while let Some(arg) = raw.next(&mut cursor) {
        if let Some((Ok(name), opt)) = arg.to_long() {
            if let Some(arg_name) = name.strip_prefix("grcov-") {
                if grcov.has_value(arg_name) {
                    ret.grcov.push(format!("--{arg_name}").into());
                    if let Some(value) = opt.or_else(|| raw.next_os(&mut cursor)) {
                        ret.grcov.push(value.to_os_string())
                    }
                }
                continue;
            } else if name == "help" {
                print_help();
            }
        } else if let Some(mut shorts) = arg.to_short() {
            while let Some(short) = shorts.next_flag() {
                if let Ok('h') = short {
                    print_help();
                }
            }
        }
        ret.cargo.push(arg.to_value_os());
    }

    ret
}

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

    let grcov_args = match Command::new("grcov").arg("-h").output() {
        Ok(ref out) if let Ok(out) = str::from_utf8(out.stdout.as_slice()) => revarg_parse(out),
        Ok(_) => Default::default(),
        Err(_) => {
            eprintln!(
                "🚧 {0} is not installed Please install {0} to continue. See {1}.",
                "grcov".yellow().italic(),
                "https://github.com/mozilla/grcov".blue().italic()
            );
            std::process::exit(1);
        }
    };

    let raw_args = clap_lex::RawArgs::from_args();
    let args = parse_args(&raw_args, &grcov_args);

    // Remove all existing profraw files
    if coverage_dir.exists() && env::var_os("CARGO_COVER_NOCLEAN").is_none_or(|v| v.is_empty()) {
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

    if !Command::new("cargo")
        .arg("test")
        .args(&args.cargo)
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
    let mut grcov_args = vec![
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
        grcov_args.extend(["--keep-only", keep_path.as_str()]);
    }

    eprintln!(
        "{} markdown coverage report",
        format!("{:>12}", "Generating").green().bold()
    );
    let mut child = Command::new("grcov")
        .args(&grcov_args)
        .arg("-t")
        .arg("markdown")
        .args(&args.grcov)
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
                    Ok(num) if num > 90f32 => cap[0].green().bold().to_string(),
                    Ok(num) if num > 75f32 => cap[0].yellow().bold().to_string(),
                    Ok(_) => cap[0].red().bold().to_string(),
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
        format!("{:>12}", "Generating").green().bold(),
        html_out_dir
            .join(["html", "index.html"].into_iter().collect::<PathBuf>())
            .to_str()
            .unwrap()
    );
    fs::create_dir_all(&html_out_dir)?;
    let child = Command::new("grcov")
        .args(&grcov_args)
        .arg("-t")
        .arg("html")
        .arg("-o")
        .arg(html_out_dir)
        .args(&args.grcov)
        .current_dir(&workspace_dir)
        .spawn()?;
    let _ = child.wait_with_output()?;

    eprintln!(
        "{} lcov coverage report ({})",
        format!("{:>12}", "Generating").green().bold(),
        lcov_file.to_str().unwrap()
    );
    /* finish with lcov since its report would be parsed by grcov */
    let child = Command::new("grcov")
        .args(&grcov_args)
        .arg("-t")
        .arg("lcov")
        .arg("-o")
        .arg(lcov_file)
        .args(&args.grcov)
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

    Ok(package_root)
}
