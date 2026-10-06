use std::path::PathBuf;

use celesta_gpu_renderer::GpuDriver;
use clap::Parser;
use clap::builder::{PossibleValuesParser, TypedValueParser};

#[derive(Debug, Parser)]
#[command(name = "celesta-editor", version, about = "Celesta video editor")]
pub struct Args {
    /// Open a project (.celesta.json) or React composition (.tsx).
    #[arg(value_name = "PROJECT", conflicts_with = "init")]
    pub path: Option<PathBuf>,

    /// Initialize a React project, preserving existing source and configuration.
    /// Defaults to the current directory; .celesta/ is refreshed.
    #[arg(long, value_name = "DIRECTORY", num_args = 0..=1, default_missing_value = ".")]
    pub init: Option<PathBuf>,

    /// Graphics API to render the preview and exports with; auto takes the
    /// first one with a GPU.
    #[arg(
        long,
        default_value_t = GpuDriver::default(),
        value_parser = PossibleValuesParser::new(GpuDriver::ALL.map(GpuDriver::as_str))
            .map(|name| name.parse::<GpuDriver>().expect("listed driver names parse"))
    )]
    pub driver: GpuDriver,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    fn parse(args: &[&str]) -> Result<Args, clap::Error> {
        Args::try_parse_from(["celesta-editor"].into_iter().chain(args.iter().copied()))
    }

    #[test]
    fn parses_open_and_init() {
        let args = parse(&[]).unwrap();
        assert_eq!((args.path, args.init), (None, None));
        assert_eq!(parse(&["film.tsx"]).unwrap().path, Some("film.tsx".into()));
        assert_eq!(parse(&["--init"]).unwrap().init, Some(".".into()));
        assert_eq!(
            parse(&["--init", "日本語 project"]).unwrap().init,
            Some("日本語 project".into())
        );
        assert_eq!(
            parse(&["--", "-film.tsx"]).unwrap().path,
            Some("-film.tsx".into())
        );
    }

    #[test]
    fn parses_driver() {
        assert_eq!(parse(&[]).unwrap().driver, GpuDriver::Auto);
        assert_eq!(
            parse(&["--driver=dx12", "film.tsx"]).unwrap().driver,
            GpuDriver::Dx12
        );
        assert_eq!(
            parse(&["--driver", "vulkan"]).unwrap().driver,
            GpuDriver::Vulkan
        );
        assert_eq!(
            parse(&["--driver=d3d12"]).unwrap_err().kind(),
            ErrorKind::InvalidValue
        );
    }

    #[test]
    fn reports_help_and_version_without_opening_the_editor() {
        for args in [&["--help"][..], &["-h"], &["--init", "--help"]] {
            assert_eq!(parse(args).unwrap_err().kind(), ErrorKind::DisplayHelp);
        }
        assert_eq!(
            parse(&["--version"]).unwrap_err().kind(),
            ErrorKind::DisplayVersion
        );
    }

    #[test]
    fn rejects_unknown_options_extra_arguments_and_open_with_init() {
        for args in [&["--unknown"][..], &["a.tsx", "b.tsx"]] {
            assert_eq!(parse(args).unwrap_err().kind(), ErrorKind::UnknownArgument);
        }
        for args in [&["--init", "a", "b"][..], &["film.tsx", "--init"]] {
            assert_eq!(parse(args).unwrap_err().kind(), ErrorKind::ArgumentConflict);
        }
    }
}
