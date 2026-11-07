mod error;
mod lexer;
mod parser;
mod runner;
mod value;

use std::env;
use runner::run::{run_file, run_prompt, run_test_file};

fn real_main() -> i32 {
    let args: Vec<String> = env::args().collect();
    let program = args[0].rsplit('/').next().unwrap_or("dsharp2");

    match args.as_slice() {
        [_, flag, path] if flag.as_str() == "--test" => match run_test_file(path) {
            Ok(report) => {
                println!("running {} test(s)", report.results.len());

                for result in &report.results {
                    match &result.outcome {
                        Ok(()) => println!("test {} ... ok", result.name),
                        Err(error) => {
                            println!("test {} ... FAILED", result.name);
                            println!("  {error}");
                        }
                    }
                }

                println!();
                if report.is_success() {
                    println!(
                        "test result: ok. {} passed; 0 failed",
                        report.passed()
                    );
                    0
                } else {
                    println!(
                        "test result: FAILED. {} passed; {} failed",
                        report.passed(),
                        report.failed()
                    );
                    1
                }
            }
            Err(error) => {
                println!("ERROR: \n{error}");
                1
            }
        },
        [_, path] => match run_file(path) {
            Ok(_) => 0,
            Err(error) => {
                println!("ERROR: \n{error}");
                1
            }
        },
        [_] => match run_prompt() {
            Ok(_) => 0,
            Err(error) => {
                println!("ERROR\n{error}");
                1
            }
        },
        _ => {
            println!("Usage: {program} [script]");
            println!("       {program} --test <script>");
            0
        }
    }
}

fn main() {
    std::process::exit(real_main());
}
