use alsa::pcm::{Access, Format, HwParams, State, IO};
use alsa::{Direction, ValueOr, PCM};
use claxon::{Block, FlacReader};
use std::env;
use std::fs::File;
use std::io::Read;

macro_rules! dbg {
($( $x:expr ), +) => {

    if cfg!(debug_assertions) {
        std::dbg!($($x), +)
    } else {
            ($($x), +)
    }

}
}

fn main() {
    let mut track = None;
    if env::args().len() == 2 {
        track = env::args().last();
    }
    let mut reader = claxon::FlacReader::open(track.unwrap()).unwrap();
    let pcm = PCM::new("default", Direction::Playback, false).unwrap();

    dbg!(reader.streaminfo());

    let hwp = HwParams::any(&pcm).unwrap();
    hwp.set_channels(reader.streaminfo().channels).unwrap();
    hwp.set_rate(reader.streaminfo().sample_rate, ValueOr::Nearest)
        .unwrap();
    hwp.set_format(match reader.streaminfo().bits_per_sample {
        16 => Format::S16LE,
        24 => Format::S24LE,
        _ => panic!(),
    })
    .unwrap();
    hwp.set_access(Access::RWInterleaved).unwrap();
    pcm.hw_params(&hwp).unwrap();

    let io = pcm.io_bytes();

    let hwp = pcm.hw_params_current().unwrap();
    let swp = pcm.sw_params_current().unwrap();
    swp.set_start_threshold(hwp.get_buffer_size().unwrap())
        .unwrap();
    pcm.sw_params(&swp).unwrap();

    play_flac(reader, &pcm, &io);

    pcm.drain().unwrap();
}

fn play_flac(mut reader: FlacReader<File>, pcm: &PCM, io: &IO<u8>) {
    let mut blocks = 0;
    let mut vec_buf = Vec::<i32>::with_capacity(reader.streaminfo().max_block_size as usize);
    let mut block = reader.blocks().read_next_or_eof(vec_buf).unwrap();
    loop {
        #[cfg(debug_assertions)]
        dbg!(blocks);
        blocks += 1;
        match block {
            None => {
                break;
            }
            Some(b) => {
                let buffer = &b
                    .stereo_samples()
                    .map(|i| [i.0.to_le_bytes(), i.1.to_le_bytes()])
                    .flatten()
                    .flatten()
                    .collect::<Vec<u8>>()[..];

                dbg!(io.writei(buffer).unwrap());

                vec_buf = b.into_buffer();
                block = reader.blocks().read_next_or_eof(vec_buf).unwrap();
                if pcm.state() != State::Running {
                    pcm.start().unwrap();
                };
            }
        }
    }
}
fn play_pcm(pcm: &PCM, io: &IO<u8>) {
    let mut buffer = [0u8; 44100];
    let mut f = File::open("../music/audio.pcm").unwrap();

    let mut time = 0;

    let mut data_read = f.read(&mut buffer).unwrap();
    dbg!(data_read, time);
    time += 1;

    while data_read >= 1024 {
        dbg!(io.writei(&buffer[..]).unwrap());
        data_read = f.read(&mut buffer).unwrap();
        dbg!(data_read, time);
        time += 1;
        if pcm.state() != State::Running {
            pcm.start().unwrap();
        };
    }
}
