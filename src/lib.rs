#[macro_use]
extern crate log;
extern crate reqwest;
extern crate json;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate indicatif;
extern crate positioned_io;

use std::path::PathBuf;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::fs;
use std::io::Read;
use std::time::Duration;
use std::sync::mpsc::channel;
use anyhow::Result;
use threadpool::ThreadPool;
use reqwest::header::{HeaderMap, HeaderValue};
use indicatif::{ProgressBar, ProgressStyle};

pub fn get_file_header(url: &str) ->Result<u64> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Content-Type", HeaderValue::from_static("application/x-www-form-urlencoded")
    );
    let resp: reqwest::blocking::Response = reqwest::blocking::Client::new()
        .get(url)
        .headers(headers)
        .send()?;
    //let status = resp.status().is_success();
    if !resp.status().is_success() {
        println!("{:#?}", resp);
        return Ok(0);
    }
    let file_length = resp.headers().get("content-length").unwrap().to_str().unwrap();
    let file_length: u64 = file_length.parse()?;
    Ok(file_length)
}
pub fn range_download_file(
    url: String,
    out_file_path: PathBuf,
    out_file_patch: PathBuf,
    thread_num: usize,
    part_size: usize,
    file_length: usize,
) -> Result<()> {
    use positioned_io::WriteAt;
    //计算需要分多少快下载 （文件大小/每快的大小)
    let part_num = file_length / part_size;

    //如果patch文件存在， 则读取然后初始化
    let mut patch = Patch::new(file_length, part_num, &out_file_patch);

    //创建目标文件和patch文件(存在与否都重新创建）
    let (mut out_file, mut patch_file) = create_file_and_patch(&out_file_path, &out_file_patch);

    //线程池
    let pool = ThreadPool::new(thread_num);
    let (tx, rx) = channel();

    let (mut start, mut end) = (0, 0);
    for n in 0..=part_num {
        if n != 0 {
            start = end + 1;
        }
        end = end + part_size;
        if end >= file_length {
            end = file_length - 1;
        }

        let heads = format!("bytes={}-{}", start, end);
        if Some(&true) == patch.is_down.get(&heads) {
            debug!("Skip Download with {}", &heads);
            continue;
        }
        debug!("Range Request Header: {} URL:{:?}", &heads, &url);
        let req = build_range_request(heads.clone());

        let tx = tx.clone();
        let url = url.clone();
        pool.execute(move || {
            let mut resp: reqwest::blocking::Response = req
                .build()
                .expect("req build exception")
                .get(&url)
                .send()
                .expect("req send exception");
            let mut buf: Vec<u8> = vec![];
            resp.copy_to(&mut buf).unwrap();

            tx.send((start, heads.clone(), buf))
                .expect("请求下载后发送到写入文件出错")
        });
    }

    let name = out_file_path.file_name().unwrap().to_str().unwrap();
    let pb = prgressbar(file_length as u64, name);
    rx.iter()
        .take(part_num + 1 - patch.is_down.len())
        .for_each(|(start, pstr,  buf)| {
            let writed = out_file.write_at(start as u64, &buf).expect("写入文件失败");
            debug!("pstr:{} buf:{}, writed:{}", pstr, buf.len(), writed);
            patch.is_down.insert(pstr, true);
            let patch_string = serde_json::to_string(&patch).expect("patch序列化出错");
            let _ = patch_file.write_at(0, patch_string.as_bytes());
            pb.inc(buf.len() as u64);
        });
    pb.finish_with_message("done");
    Ok(())
}



#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Patch {
    pub file_size: usize,
    pub part_num: usize,
    pub is_down: HashMap<String, bool>,
}

impl Patch {
    pub fn new(file_length: usize, part_num: usize, out_file_patch: &PathBuf) -> Patch {
        let mut patch = Patch {
            file_size: file_length,
            part_num,
            is_down: HashMap::default(),
        };
        //已存在， 则用文件初始化
        if fs::metadata(&out_file_patch).is_ok() {
            let mut part_file_str = String::new();
            let _ = File::open(out_file_patch)
                .unwrap()
                .read_to_string(&mut part_file_str);
            if part_file_str.len() > 0 {
                let file_patch: Patch =
                    serde_json::from_str(part_file_str.as_str()).expect("读取patch文件出错");
                //调整过参数， 原来记录不能用
                if file_patch.file_size == file_length && file_patch.part_num == part_num {
                    patch = file_patch;
                }
            }
            debug!("Load Patch: {:?}", &patch)
        }
        patch
    }
}


fn create_file_and_patch(out_file_path: &PathBuf, out_file_patch: &PathBuf) -> (File, File) {
    //目标文件， 存在就打开
    let out_file = if fs::metadata(out_file_path).is_ok() {
        OpenOptions::new().write(true).open(out_file_path).unwrap()
    } else {
        File::create(out_file_path).expect("创建目标文件失败")
    };
    //patch文件
    let patch_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .append(false)
        .open(out_file_patch)
        .unwrap();
    (out_file, patch_file)
}

fn build_range_request(heads: String) -> reqwest::blocking::ClientBuilder {
    let mut header_map = HeaderMap::new();
    header_map.insert(
        "Range",
        HeaderValue::from_str(heads.as_str()).expect("Header token error"),
    );

    let req = reqwest::blocking::ClientBuilder::new()
        .timeout(Duration::from_secs(100))
        .default_headers(header_map);

    req
}

fn prgressbar(count: u64, name: &str) -> ProgressBar {
    let pb = ProgressBar::new(count);
    pb.set_style(ProgressStyle::default_bar().template(
        (format!("{}", name) + "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})").as_str(),
    ));
    pb.inc(0);
    pb
}
