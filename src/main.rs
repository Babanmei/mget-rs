use mget_rs::{range_download_file, get_file_header};
use std::path::PathBuf;
use std::str::FromStr;

fn main() {
    let yaml = clap::load_yaml!("cli.yml");
    let matches = clap::App::from(yaml).get_matches();

    let threads = matches.value_of("threads");
    let part_size_opt = matches.value_of("partSize");
    let url = matches.value_of("remote");
    if url.is_none() {
        println!("please input url");
        return;
    }
    let url = url.unwrap();
    let local_path = matches.value_of("local");
    //set default
    let thread_num = if threads.is_none() { 3 } else { threads.unwrap().parse().unwrap() };
    let part_size = if part_size_opt.is_none() { 10 } else { part_size_opt.unwrap().parse().unwrap() };
    let part_size = part_size * 1024 * 1024;
    let local_path = if local_path.is_none() { "." }else{ local_path.unwrap() };

    let x: Vec<&str> = url.split("/").collect();
    let file_name = x.last().expect("截取文件名出错");
    let out_file_patch = PathBuf::from_str(local_path)
        .unwrap()
        .join(String::from(*file_name) + ".patch");
    let out_file_path = PathBuf::from_str(local_path).unwrap().join(*file_name);

    let file_length = get_file_header(url).unwrap();

    let _ = range_download_file(
        url.to_string(),
        out_file_path,
        out_file_patch,
        thread_num,
        part_size,
        file_length as usize
    );
}
