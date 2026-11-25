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

use std::collections::HashMap;

fn parse_value_name(line: &mut &str) -> Option<String> {
    if line.ends_with('>')
        && let Some(idx) = line.rfind('<')
    {
        let ret = &line[idx + 1..line.len() - 1];
        *line = line[..idx - 1].trim();
        return Some(ret.to_string());
    }
    None
}

fn parse_long_opt(line: &str) -> Option<String> {
    let mut long = None;
    for name in line.split(',') {
        if name.starts_with("--")
            && let name = name[2..].trim()
        {
            if !name.chars().all(|c| c.is_alphanumeric() || c == '-') {
                return None;
            }
            long = Some(name.to_string());
        } else if name.starts_with("-") {
            /* ignore short option */
            continue;
        } else {
            return None;
        }
    }
    long
}

#[derive(Default)]
pub struct RevParsedArgs(HashMap<String, Option<String>>);

impl std::ops::Deref for RevParsedArgs {
    type Target = HashMap<String, Option<String>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl RevParsedArgs {
    #[inline]
    pub fn has_value(&self, arg: &str) -> bool {
        self.0.get(arg).is_some_and(|v| v.is_some())
    }
}

impl<'a> IntoIterator for &'a RevParsedArgs {
    type Item = <&'a <RevParsedArgs as std::ops::Deref>::Target as IntoIterator>::Item;
    type IntoIter = <&'a <RevParsedArgs as std::ops::Deref>::Target as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

/// retrieve arguments that look like: "-k, --keep-only <PATH>"
pub fn revarg_parse(help_message: &str) -> RevParsedArgs {
    let mut args = RevParsedArgs::default();
    for line in help_message.lines() {
        let mut line = line.trim();
        if !line.starts_with('-') {
            continue;
        }

        let value_name = parse_value_name(&mut line);
        if let Some(long) = parse_long_opt(line) {
            args.0.insert(long, value_name);
        }
    }
    args
}

#[cfg(test)]
mod tests {
    use super::revarg_parse;
    use std::process::Command;

    #[test]
    fn parse_args() {
        let args = match Command::new("grcov").arg("-h").output() {
            Ok(ref output) if let Ok(stdout) = str::from_utf8(output.stdout.as_slice()) => {
                revarg_parse(stdout)
            }
            Ok(_) => {
                panic!("invalid grcov output")
            }
            Err(_) => panic!("grcov not found"),
        };
        for arg in &args {
            eprintln!("parsed arg: {} with value: {:?}", arg.0, arg.1);
        }
        for opt in ["ignore", "keep-only"] {
            assert!(args.contains_key(opt), "option \"{opt}\" not detected");
        }
    }
}
