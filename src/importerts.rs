use std::{
    fs::File, path::PathBuf, time::{Duration, Instant},
    thread::JoinHandle, sync::{Arc, Mutex},
    thread, sync::mpsc::{Sender, Receiver, channel}
};

// Multimedia format handling
use symphonia::core::{
    io::MediaSourceStream, formats::FormatOptions, meta::MetadataOptions,
    probe::Hint, codecs::DecoderOptions, audio::SampleBuffer
};

use crate::types::*;

// Multithreaded variants ---------------------------------------------------------------------------------------------------
// Imports the 4 separated tracks from a directory; The names of the .mp3 files must be {bass, drums, vocals, other}.mp3
// Returns TrackBuffers and true if the directory contains the original stems.
pub fn mt_import_from_directory(path: &String) -> Result<(Vec<TrackBuffer>, bool), String> {
    println!("Looking into {} for separated stems...", path);
    let mut is_original: bool = false;

    // Check this directory has all the required files
    let dir_contents = match std::fs::read_dir(path) {
        Ok(d) => { d }
        Err(_) => { return Result::Err(format!("import_from_directory():\n\tread_dir({}): Failed to open directory (insufficient access rights?)", path)); }
    };

    let mut paths: Vec<Option<PathBuf>> = Vec::new();
    paths.resize(4, Option::None);

    let required_files: Vec<&str> = vec!["bass.mp3", "drums.mp3", "vocals.mp3", "other.mp3"];
    let mut hits = 0;
    
    // Try finding all four files in `path`
    for e in dir_contents {
        let entry = match e {
            Ok(r)  => { r }
            Err(_) => { continue; } // Error entries will be silently skipped
        };

        let mut req_files_it = required_files.iter();
        // Get entry's path and filename
        let item_path = &entry.path();
        let item_name = item_path.file_name().unwrap();
        
        // Check if the `.original` hidden file exists
        if item_name.eq(".original") {
            is_original = true;
            print!("Detected that this is the original source directory.");
            continue;
        }

        // Search for the filename in `required_files`
        match req_files_it.position(|&x| x == item_name) {
            Some(index) => {
                paths[index] = Option::Some(item_path.clone());
                hits += 1;
            }
            None => { continue; }
        }
        if hits == 4 { break; }
    }

    if hits != 4 {
        return Result::Err(format!("import_from_directory(): Could not find all separated stems (found {}/4)", hits));
    }

    // Use 4 MPSC pairs, one for each thread
    let mut receivers: Vec<Receiver<i32>> = vec![];
    receivers.reserve(4);

    // Create 4 shared vectors; Each thread should have its own Arc
    let mut shared_buffers: Vec<Arc<Mutex<TrackBuffer>>> = vec![];
    shared_buffers.reserve(4);

    // Spawn threads
    let mut handles: Vec<JoinHandle<_>> = vec![];
    handles.reserve(4);

    for filename in paths {
        let filename = filename.unwrap();
        let filename_string: String = filename.to_str().unwrap().to_string();

        // Create a buffer behind an Arc and keep a copy
        let new_buffer: Arc<Mutex<TrackBuffer>> = Arc::new(Mutex::<TrackBuffer>::new(vec![]));
        shared_buffers.push(
            Arc::clone(&new_buffer)
        );

        // Create a channel and only keep the receiver
        let (tx, rx) = channel();
        receivers.push(rx);

        // Start thread
        handles.push(
            thread::spawn(move || { mt_import_track(
                &filename_string, 
                tx,
                Arc::clone(&new_buffer)) }    
            )
        );
    }

    // Create a vector to store each thread's state
    let mut samples_decoded: Vec<(i32, i32)> = vec![];
    samples_decoded.resize(4, (-100, 0));

    let mut threads_finished: u32 = 0;
    while threads_finished < 4 {
        for i in 0..4 {
            // Check if the thread finished
            if handles[i].is_finished() { 
                threads_finished += 1; 
                continue;
            }

            // Read thread's channel
            match receivers[i].recv() {
                Ok(r) => {
                    samples_decoded[i] = (samples_decoded[i].1, r);
                }
                Err(_) => {
                    println!("\nWarning: Thread {} is not responding.", i);
                    samples_decoded[i] = (-6, -6);
                    threads_finished += 1;
                }
            }
        }
        // Print state
        print!("\r Decoding... [ ");
        for i in 0..4 {
            if samples_decoded[i].1 < 1 { print!("ER\t"); }
            else if samples_decoded[i].0 == samples_decoded[i].1 { print!("OK\t"); }
            else { print!("{}\t", samples_decoded[i].1); }
        }
        print!("]");

        // Sleep
        thread::sleep(Duration::from_millis(10));
    }

    print!("\rDone decoding.                                           \n");

    // Return the shared buffers
    let mut tracks_interleaved_vec = vec![];
    for i in 0..4 {
        let track: TrackBuffer = shared_buffers[i].lock().unwrap().to_vec();
        tracks_interleaved_vec.push(track);
    }

    return Result::Ok((tracks_interleaved_vec, is_original))
}


// Multithread variant. This function should be executed by a single thread.
// Loads a track from a file and returns a TrackBuffer (vector of 32-bit floats); Channels are interleaved in the output
fn mt_import_track(path: &String, tx: Sender<i32>, buffer: Arc<Mutex<TrackBuffer>>) {
    // Check this file is an .mp4
    let f = File::open(path);
    if f.is_err() { 
        println!("\nimport_from_file(): Could not open {}.", path);
        tx.send(-1);
        return;
    }
    let f = f.unwrap();

    // Media Source Stream, metadata and format readers
    let mss = MediaSourceStream::new(Box::new(f), Default::default());
    let meta_opts:  MetadataOptions = Default::default();
    let fmt_opts:   FormatOptions   = Default::default();

    // Create a hint
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    // Probe
    let probe = match symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts) {
        Result::Ok(p)  => { p }
        Result::Err(_) => {  
            println!("\nsymphonia::default::get_probe(): Unsupported format");
            tx.send(-2);
            return;  
        }
    };

    let mut format_reader = probe.format;

    let track_count = format_reader.tracks().len();
    if track_count != 1 { 
        println!("\nimport_from_file(): This file doesn't contain just one audio track (containts {})", track_count);
        tx.send(-3);
        return;
    }

    // Create a decoder 
    let track = format_reader.tracks().get(0).unwrap();
    let dec_opts: DecoderOptions = Default::default();
    let mut decoder = match symphonia::default::get_codecs().make(&track.codec_params, &dec_opts){
        Result::Ok(d)  => { d }
        Result::Err(_) => {  
            println!("import_from_file():\n\tget_codecs(): Unsupported format.");
            tx.send(-4);
            return;
        }
    };

    // Start decoding
    let mut sample_count: usize = 0;
    let mut temp_buffer = Option::None;

    // Get the buffer behind the mutex; The buffer will be automatically unlocked at the end of the function
    let mut return_buffer = buffer.lock().unwrap(); // .get_mut() implies .lock()

    // Read the first packet
    loop {
        let packet = match format_reader.next_packet()  {
            Ok(packet) => packet,
            Err(_) => { 
                println!("\nimport_from_file(): The first packet caused an error.");
                tx.send(-5);
                return; 
            }
        };
    
        // Consume any new metadata that has been read since the last packet.
        while !format_reader.metadata().is_latest() {
            // Pop the old head of the metadata queue.
            format_reader.metadata().pop();

            // Consume the new metadata at the head of the metadata queue.
        }

        // Decode to audio sample
        match decoder.decode(&packet) {
            Ok(new_buffer) => {
                if temp_buffer.is_none() {
                    let spec = *new_buffer.spec();
                    let duration = new_buffer.capacity() as u64;
                    temp_buffer = Some(SampleBuffer::<f32>::new(duration, spec));
                }

                if let Some(buf) = &mut temp_buffer {
                    buf.copy_interleaved_ref(new_buffer);

                    return_buffer.extend_from_slice(buf.samples());
                    sample_count += buf.samples().len();
                }

                break;
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => { break; }
            Err(_) => { break; }
        }
    }

    loop {
        let packet = match format_reader.next_packet() {
            Ok(packet) => packet,
            Err(_) => { break; }
        };
        
        // Skip packets of different tracks
        // Probably not needed since we checked there is only one track in the file
        // if packet.track_id() != track_id {continue;}

        // Decode to audio sample
        match decoder.decode(&packet) {
            Ok(new_buffer) => {
                if let Some(buf) = &mut temp_buffer {
                    buf.copy_interleaved_ref(new_buffer);
                    sample_count += buf.samples().len();

                    return_buffer.extend_from_slice(buf.samples());
                    if sample_count % 64 == 0 { tx.send(sample_count as i32); }
                }
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => { break; }
            Err(_) => { break; }
        }
    }

    tx.send(sample_count as i32);
    tx.send(sample_count as i32);
    return;
}

// ---------------------------------------------------------------------------------------------------------------------------------

// Imports the 4 separated tracks from a directory; The names of the .mp3 files must be {bass, drums, vocals, other}.mp3
// Returns TrackBuffers and true if the directory contains the original stems.
pub fn import_from_directory(path: &String) -> Result<(Vec<TrackBuffer>, bool), String> {
    println!("Looking into {} for separated stems...", path);
    let mut is_original: bool = false;

    // Check this directory has all the required files
    let dir_contents = match std::fs::read_dir(path) {
        Ok(d) => { d }
        Err(_) => { return Result::Err(format!("import_from_directory():\n\tread_dir({}): Failed to open firectory (insufficient access rights?)", path)); }
    };

    let mut paths: Vec<Option<PathBuf>> = Vec::new();
    paths.resize(4, Option::None);

    let required_files: Vec<&str> = vec!["bass.mp3", "drums.mp3", "vocals.mp3", "other.mp3"];
    let mut hits = 0;
    
    // Try finding all four files in `path`
    for e in dir_contents {
        let entry = match e {
            Ok(r)  => { r }
            Err(_) => { continue; } // Error entries will be silently skipped
        };

        let mut req_files_it = required_files.iter();
        // Get entry's path and filename
        let item_path = &entry.path();
        let item_name = item_path.file_name().unwrap();
        
        // Check if the `.original` hidden file exists
        if item_name.eq(".original") {
            is_original = true;
            print!("Detected that this is the original source directory.");
            continue;
        }

        // Search for the filename in `required_files`
        match req_files_it.position(|&x| x == item_name) {
            Some(index) => {
                paths[index] = Option::Some(item_path.clone());
                hits += 1;
            }
            None => { continue; }
        }
        if hits == 4 { break; }
    }

    if hits != 4 {
        return Result::Err(format!("import_from_directory(): Could not find all separated stems (found {}/4)", hits));
    }

    // Import each file's track
    let mut tracks_interleaved_vec: Vec<TrackBuffer> = vec![];
    tracks_interleaved_vec.reserve(4);

    for filename in paths { // PARALLEL
        let filename = filename.unwrap();
        let filename_string: String = filename.to_str().unwrap().to_string();
        match import_track(&filename_string) {
            Ok(ret_buffer) => {
                tracks_interleaved_vec.push(ret_buffer);
            }
            Err(e) => { return Result::Err(format!("import_from_directory():\n\t{}", e)); }
        }
    } 

    return Result::Ok((tracks_interleaved_vec, is_original))
}


// Loads a track from a file and returns a TrackBuffer (vector of 32-bit floats); Channels are interleaved in the output
pub fn import_track(path: &String) -> Result<TrackBuffer, String> {
    // Check this file is an .mp4
    let f = File::open(path);
    if f.is_err() { return Result::Err(format!("import_from_file(): Could not open {}.", path)); }
    let f = f.unwrap();

    // Media Source Stream, metadata and format readers
    let mss = MediaSourceStream::new(Box::new(f), Default::default());
    let meta_opts:  MetadataOptions = Default::default();
    let fmt_opts:   FormatOptions   = Default::default();

    // Create a hint
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    // Probe
    let probe = match symphonia::default::get_probe().format(&hint, mss, &fmt_opts, &meta_opts) {
        Result::Ok(p)  => { p }
        Result::Err(_) => {  return Result::Err(String::from("symphonia::default::get_probe(): Unsupported format"));  }
    };

    let mut format_reader = probe.format;

    let track_count = format_reader.tracks().len();
    if track_count != 1 { return Result::Err(format!("import_from_file(): This file doesn't contain just one audio track (containts {})", track_count)); }

    // Create a decoder 
    let track = format_reader.tracks().get(0).unwrap();
    let dec_opts: DecoderOptions = Default::default();
    let mut decoder = match symphonia::default::get_codecs().make(&track.codec_params, &dec_opts){
        Result::Ok(d)  => { d }
        Result::Err(_) => {  return Result::Err(format!("import_from_file():\n\tget_codecs(): Unsupported format."));  }
    };
    //let track_id = track.id;

    // Start decoding
    let mut sample_count: usize = 0;
    let mut temp_buffer = Option::None;
    let mut return_buffer: Vec<f32> = vec![];

    let decode_start = Instant::now();

    // Read the first packet
    loop {
        let packet = match format_reader.next_packet()  {
            Ok(packet) => packet,
            Err(_) => { return Result::Err(String::from("import_from_file(): The first packet caused an error.")); }
        };
    
        // Consume any new metadata that has been read since the last packet.
        while !format_reader.metadata().is_latest() {
            // Pop the old head of the metadata queue.
            format_reader.metadata().pop();

            // Consume the new metadata at the head of the metadata queue.
        }

        // Decode to audio sample
        match decoder.decode(&packet) {
            Ok(new_buffer) => {
                if temp_buffer.is_none() {
                    let spec = *new_buffer.spec();
                    let duration = new_buffer.capacity() as u64;
                    temp_buffer = Some(SampleBuffer::<f32>::new(duration, spec));
                }

                if let Some(buf) = &mut temp_buffer {
                    buf.copy_interleaved_ref(new_buffer);

                    return_buffer.extend_from_slice(buf.samples());
                    sample_count += buf.samples().len();
                }

                break;
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => { break; }
            Err(_) => { break; }
        }
    }

    loop {
        let packet = match format_reader.next_packet() {
            Ok(packet) => packet,
            Err(_) => { break; }
        };
        
        // Skip packets of different tracks
        // Probably not needed since we checked there is only one track in the file
        // if packet.track_id() != track_id {continue;}

        // Decode to audio sample
        match decoder.decode(&packet) {
            Ok(new_buffer) => {
                if let Some(buf) = &mut temp_buffer {
                    buf.copy_interleaved_ref(new_buffer);
                    sample_count += buf.samples().len();

                    return_buffer.extend_from_slice(buf.samples());
                    if sample_count % 64 == 0 {
                        print!("\rDecoding... {}", match (sample_count/32768) % 4 {
                            0_usize => "|",
                            1_usize => "/",
                            2_usize => "-",
                            _ => "\\"
                        });
                    }
                }
            }
            Err(symphonia::core::errors::Error::DecodeError(_)) => { break; }
            Err(_) => { break; }
        }
    }
    let decode_time = decode_start.elapsed();

    match sample_count == 0 {
        true  => { return Result::Err( String::from("import_from_file(): No problems detected but nothing was decoded.")); }
        false => {
            println!("\r {}:\n\tDecoded {} samples per channel.\t[{} ms]", path, sample_count/2, decode_time.as_millis());
            return Result::Ok(return_buffer);
        }
    }
}

