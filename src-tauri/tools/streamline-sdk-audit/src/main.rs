use std::ffi::OsString;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use streamline_sdk_audit::{audit, baseline, Roots};

const USAGE: &str = "streamline-sdk-audit --reference-dir <absolute directory> --sdk-dir <absolute directory> --loader <absolute file>\nRead-only P0 evidence audit. Does not launch Ryubing or load DLLs.\nJSON on stdout; exit 1 = invalid/mismatching inputs or unsupported host, 2 = evidence matches but P0 remains blocked.\nThe reference directory is the investigated fork, NOT an approved original Ryubing launch target.";

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Option<Roots>, String> {
    let mut args = args.into_iter().peekable();
    if args
        .peek()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        args.next();
        return if args.next().is_none() {
            Ok(None)
        } else {
            Err("--help takes no other arguments".into())
        };
    }
    let (mut reference, mut sdk, mut loader) = (None, None, None);
    while let Some(flag) = args.next() {
        let slot = match flag.to_str() {
            Some("--reference-dir") => &mut reference,
            Some("--sdk-dir") => &mut sdk,
            Some("--loader") => &mut loader,
            _ => return Err(format!("unknown argument: {}", flag.to_string_lossy())),
        };
        if slot.is_some() {
            return Err(format!("duplicate argument: {}", flag.to_string_lossy()));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {}", flag.to_string_lossy()))?;
        let path = PathBuf::from(value);
        if !path.is_absolute() {
            return Err(format!("expected absolute path: {}", path.display()));
        }
        *slot = Some(path);
    }
    Ok(Some(Roots {
        reference: reference.ok_or("missing --reference-dir")?,
        sdk: sdk.ok_or("missing --sdk-dir")?,
        loader: loader.ok_or("missing --loader")?,
    }))
}

fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let Some(roots) = parse(std::env::args_os().skip(1))? else {
        println!("{USAGE}");
        return Ok(0);
    };
    let report = audit(&baseline()?, &roots);
    let mut output = io::stdout().lock();
    serde_json::to_writer_pretty(&mut output, &report)?;
    writeln!(output)?;
    Ok(report.exit_code())
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_cli_cannot_silently_choose_paths() {
        for args in [
            vec![],
            vec!["--launch"],
            vec!["--reference-dir"],
            vec!["--reference-dir", "relative"],
            vec!["--help", "--launch"],
        ] {
            assert!(parse(args.into_iter().map(OsString::from)).is_err());
        }
        assert!(parse([OsString::from("--help")]).unwrap().is_none());
    }
}
