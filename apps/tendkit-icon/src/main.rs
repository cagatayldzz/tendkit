use std::{env, error::Error, process::ExitCode};

fn main() -> ExitCode {
  match run() {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprintln!("Icon rendering failed: {error}");
      ExitCode::FAILURE
    }
  }
}

fn run() -> Result<(), Box<dyn Error>> {
  let mut developer = false;
  let mut output = None;
  for argument in env::args().skip(1) {
    if argument == "--dev" {
      developer = true;
    } else if argument.starts_with('-') || output.is_some() {
      return Err("Usage: tendkit-icon [--dev] <output.png>".into());
    } else {
      output = Some(argument);
    }
  }
  let output = output.ok_or("Usage: tendkit-icon [--dev] <output.png>")?;
  let badge = if developer { include_str!("../badge.svg") } else { "" };
  let svg = include_str!("../icon.svg").replace("<!-- DEV_BADGE -->", badge);
  let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default())?;
  let mut pixmap = resvg::tiny_skia::Pixmap::new(1024, 1024).ok_or("Cannot allocate icon bitmap")?;
  resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
  pixmap.save_png(&output)?;
  println!("Wrote {output}");
  Ok(())
}
