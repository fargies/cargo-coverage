/*
** Copyright (C) 2025 Sylvain Fargier
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

/* to eliminate this from coverage stats */
mod tests {
    use std::process::Command;

    const COVER_BIN: &str = std::env!("CARGO_BIN_EXE_cargo-cover");

    #[test]
    fn run_cli() -> Result<(), std::io::Error> {
        /* execute the test twice, once through `cargo test` then with `COVER_BIN` */
        match std::env::var("RUN_CLI").ok() {
            None => assert!(Command::new(COVER_BIN).env("RUN_CLI", "1").status()?.success()),
            Some(value) if value == "1" => assert!(Command::new(COVER_BIN).env("RUN_CLI", "2").status()?.success()),
            _ => ()
        }
        Ok(())
    }
}
