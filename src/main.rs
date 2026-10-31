mod parser;
mod runner;
mod value;

use std::env;
use runner::run::{run_file, run_prompt};

fn real_main() -> i32{
    let args:Vec<String> = env::args().collect();
    if args.len() > 2 {
        println!("Usage: jlox [script]");
        return 0;
    } 
    else if args.len() == 2{

        match run_file(&args[1]){
            Ok(_) => return 0,
            Err(msg) => println!("ERROR: \n{}", msg)
        }

    } 
    
    else {

        match run_prompt() {
            Ok(_) => return 0,
            Err(msg) => {
                println!("ERROR\n{}", msg)
            }
        }

    }

    return 0;
}


fn main() {
    std::process::exit(real_main());
}