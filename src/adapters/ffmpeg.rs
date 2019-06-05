use super::*;
use super::spawning::map_exe_error;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::io::BufReader;
use std::process::*;
use failure::*;
// todo:
// maybe todo: read list of extensions from
//ffmpeg -demuxers | tail -n+5 | awk '{print $2}' | while read demuxer; do echo MUX=$demuxer; ffmpeg -h demuxer=$demuxer | grep 'Common extensions'; done 2>/dev/null
static EXTENSIONS: &[&str] = &["mkv", "mp4", "avi"];

lazy_static! {
    static ref METADATA: AdapterMeta = AdapterMeta {
        name: "ffmpeg".to_owned(),
        version: 1,
        matchers: EXTENSIONS
            .iter()
            .map(|s| Matcher::FileExtension(s.to_string()))
            .collect(),
    };
}

pub struct FFmpegAdapter;

impl FFmpegAdapter {
    pub fn new() -> FFmpegAdapter {
        FFmpegAdapter
    }
}
impl GetMetadata for FFmpegAdapter {
    fn metadata<'a>(&'a self) -> &'a AdapterMeta {
        &METADATA
    }
}

#[derive(Serialize, Deserialize)]
struct FFprobeOutput {
    streams: Vec<FFprobeStream>,
}
#[derive(Serialize, Deserialize)]
struct FFprobeStream {
    codec_type: String, // video,audio,subtitle
}
impl FileAdapter for FFmpegAdapter {
    fn adapt(&self, inp_fname: &Path, oup: &mut dyn Write) -> Fallible<()> {
        let spawn_fail = |e| map_exe_error(e, "ffprobe", "Make sure you have ffmpeg installed.");
        let has_subtitles = {
            let probe = Command::new("ffprobe")
                .args(vec![
                    "-v",
                    "error",
                    "-select_streams",
                    "s",
                    "-of",
                    "json",
                    "-show_entries",
                    "stream=codec_type",
                ])
                .arg("-i")
                .arg(inp_fname)
                .output().map_err(spawn_fail)?;
            if !probe.status.success() {
                return Err(format_err!("ffprobe failed: {:?}", probe.status));
            }
            println!("{}", String::from_utf8_lossy(&probe.stdout));
            let p: FFprobeOutput = serde_json::from_slice(&probe.stdout)?;
            (p.streams.iter().count() > 0)
        };
        {
            let mut probe = Command::new("ffprobe")
                .args(vec![
                    "-v",
                    "error",
                    "-show_format",
                    "-show_streams",
                    "-of",
                    "flat",
                    // "-show_data",
                    "-show_error",
                    "-show_programs",
                    "-show_chapters",
                    // "-count_frames",
                    //"-count_packets",
                ])
                .arg("-i")
                .arg(inp_fname)
                .stdout(Stdio::piped())
                .spawn()?;
            for line in BufReader::new(probe.stdout.as_mut().unwrap()).lines() {
                writeln!(oup, "metadata: {}", line?)?;
            }
            let exit = probe.wait()?;
            if !exit.success() {
                return Err(format_err!("ffprobe failed: {:?}", exit));
            }
        }
        if has_subtitles {
            let mut cmd = Command::new("ffmpeg");
            cmd.arg("-hide_banner")
                .arg("-loglevel")
                .arg("panic")
                .arg("-i")
                .arg(inp_fname)
                .arg("-f")
                .arg("webvtt")
                .arg("-");
            let mut cmd = cmd.stdout(Stdio::piped()).spawn().map_err(spawn_fail)?;
            let stdo = cmd.stdout.as_mut().expect("is piped");
            let time_re = Regex::new(r".*\d.*-->.*\d.*").unwrap();
            let mut time: String = "".to_owned();
            for line in BufReader::new(stdo).lines() {
                let line = line?;
                // 09:55.195 --> 09:56.730
                if time_re.is_match(&line) {
                    time = line.to_owned();
                } else {
                    if line.len() == 0 {
                        oup.write(b"\n")?;
                    } else {
                        writeln!(oup, "{}: {}", time, line)?;
                    }
                }
            }
        }
        Ok(())
    }
}