use alsa::pcm::{Access, Format, HwParams, State};
use alsa::{Direction, ValueOr, PCM};
use std::fs::File;
use std::io::Read;

fn main() {
    let pcm = PCM::new("default", Direction::Playback, false).unwrap();

    let hwp = HwParams::any(&pcm).unwrap();
    hwp.set_channels(2).unwrap();
    hwp.set_rate(44100, ValueOr::Nearest).unwrap();
    hwp.set_format(Format::S32LE).unwrap();
    hwp.set_access(Access::RWInterleaved).unwrap();
    pcm.hw_params(&hwp).unwrap();

    let io = pcm.io_bytes();

    let hwp = pcm.hw_params_current().unwrap();
    let swp = pcm.sw_params_current().unwrap();
    swp.set_start_threshold(hwp.get_buffer_size().unwrap())
        .unwrap();
    pcm.sw_params(&swp).unwrap();

    let mut buffer = [0u8; 44100*4*2];
    let mut f = File::open("audio_1.pcm").unwrap();

    let mut time = 0;

    let mut data_read =  f.read(&mut buffer).unwrap();
    dbg!(data_read, time);
    time += 1;

    while data_read >= 44100*4*2 {
        assert_eq!(io.writei(&buffer[..]).unwrap(), 44100);
        data_read = f.read(&mut buffer).unwrap();
        dbg!(data_read, time);
        time += 1;
        if pcm.state() != State::Running {
            pcm.start().unwrap();
        };
    }
    pcm.drain().unwrap();
}
