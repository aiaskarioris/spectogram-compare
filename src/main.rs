use std::fs::File;
use std::io::Write;
use std::env;

use speccomp::types::*;
use speccomp::importerts::*;
use speccomp::spectograms::*;

use std::time::Instant; // for benchmarking

// Receives two directories as input arguments and compares the audio files located inside them.
// Both directories must containt the four X-UMX targets: Bass, Drums, Vocals & Other
fn main() {
    let args: Vec<String>  = env::args().collect();

    if (args.len() != 3) && (args.len() != 4) {
        println!("usage: spec-compare source1 source2 [--serial]\n    A source can be either a file with multiple tracks or a directory with separated stems.\n");
        return;
    }

    println!("\n=== Spectogram Compare for X-UMX =======================================================================================");
    println!(  "  Aias Karioris, 2023-2025\n");

    // For testing purposes, serial execution is available and enabled with the "--serial" flag
    let in_parallel: bool = match args.get(3) {
        Option::Some(s) => { s != "--serial" }
        Option::None => { true }
    };
    if !in_parallel { println!("Serial execution is enabled."); }

    // Start a timer
    let start_time = Instant::now();

    // Import files; every track will be loaded into `input_tracks`.
    let mut input_tracks: Vec<TrackBuffer> = vec![];
    match in_parallel {
        true => {
            // Load 4+4 tracks in parallel
            match mt_import_from_directory(&args[1]) {
                Ok(mut o)  => { input_tracks.append(&mut o); }
                Err(e) => { println!("{e}"); panic!("{e}"); }
            }
        
            match mt_import_from_directory(&args[2]) {
                Ok(mut o)  => { input_tracks.append(&mut o); }
                Err(e) => { println!("{e}"); panic!("{e}"); }
            };
        }
        
        false => {
            // Load everything sequentially
            match import_from_directory(&args[1]) {
                Ok(mut o)  => { input_tracks.append(&mut o); }
                Err(e) => { println!("{e}"); panic!("{e}"); }
            }

            match import_from_directory(&args[2]) {
                Ok(mut o)  => { input_tracks.append(&mut o); }
                Err(e) => { println!("{e}"); panic!("{e}"); }
            }
        }
    }
    
    // Create a look-up vector with target names
    let stem_names: Vec<String> = vec![
        String::from("Bass"), 
        String::from("Drums"), 
        String::from("Volcals"), 
        String::from("Other")
    ];

    println!("");
    let fft_size: u32 = 4096;

    // Calculate spectograms
    let mut spectograms_ret = match in_parallel {
        // All spectograms are calculated in parallel
        true  => { mt_track_to_spec(fft_size, input_tracks) }

        // Sequential...
        false => {
            // Create a return buffer and allocate memory for it
            let mut ret: Vec<StereoSpectogram> = vec![];
            ret.reserve(input_tracks.len());

            for i in &input_tracks {
                ret.push(track_to_spec(fft_size, i));
            }
            ret
        }
    };
    

    // Unwrap
    let mut spectograms_1: Vec<StereoSpectogram> = vec![];
    for _ in 0..4 {
        spectograms_1.push(spectograms_ret.pop().unwrap());
    }

    let mut spectograms_2: Vec<StereoSpectogram> = vec![];
    for _ in 0..4 {
        spectograms_2.push(spectograms_ret.pop().unwrap());
    }

    // Compare spectograms
    // Two methods are used: In "Time Mode" all bin differences influene the final result in the same way
    // In "Frequency Mode" bin differences of higher frequencies influence the final result less, since they are less
    // noticable by the human ear. 
    println!("");
    let mut time_mean_error: Vec<f32> = vec![];
    let mut freq_mean_error: Vec<f32> = vec![];

    // Vectors for graph exporting
    let mut graphdata_time: Vec<GraphData> = vec![];  
    let mut graphdata_freq: Vec<GraphData> = vec![];
    for i in 0..4 {
        // Comparison through time
        match time_compare_spectogram(fft_size/2, &spectograms_1[i], &spectograms_2[i]) {
            Ok((v, e)) => {            
                time_mean_error.push(e);
                graphdata_time.push(
                    GraphData::new(v, stem_names[i].clone())
                );
            }
            Err(e) => { panic!("{e}") }
        }

        // Comparison through frequencies
        match freq_compare_spectogram(fft_size/2, &spectograms_1[i], &spectograms_2[i]) {
            Ok((v, e)) => { 
                graphdata_freq.push(
                    GraphData::new(v, stem_names[i].clone())    
                );
                freq_mean_error.push(e);
            }
            Err(e) => { panic!("{e}") }
        }
    }

    // Calculate final results by getting the mean error from all tracks
    let mut time_me: f32 = 0.0;
    let mut freq_me: f32 = 0.0;
    for i in 0..4 {
        time_me += time_mean_error[i];
        freq_me += freq_mean_error[i];
    }
    time_me /= 4.0;
    freq_me /= 4.0;

    // Stop the timer and display execution time
    println!("\rDone processing! Time elapsed: {:.2} ms\n", start_time.elapsed().as_millis());

    // Display final results
    print!("\n-- Final Results ----------------------------------------\n");
    print!("     |   Vocals    Drums    Bass    Other\t|  Total\n");
    print!("Time |   {:.4}    {:.4}    {:.4}   {:.4}\t|   {:.3}\n", 
        time_mean_error[0], time_mean_error[1], time_mean_error[2], time_mean_error[3], time_me);
    print!("Freq |   {:.4}    {:.4}    {:.4}   {:.4}\t|   {:.3}\n\n", 
        freq_mean_error[0], freq_mean_error[1], freq_mean_error[2], freq_mean_error[3], freq_me); 
}

// --- Unused functions ------------------------------------------------------------------------------
// Exports comparison results into a csv file
fn export_error_csv(path: &String, data: &Vec<f32>) -> Result<(), String> {
    // Create the buffer first
    let mut write_buffer: Vec<u8> = vec![];
    write_buffer.reserve(data.len() * 8);

    // Open file
    let f = File::create(path);
    if f.is_err() { return Result::Err(format!("export_error_csv(): Could not create {}.", path)); }
    let mut f = f.unwrap();

    // Write and close
    match f.write(&write_buffer) {
        Ok(_) => {}
        Err(e) => {
            return Result::Err(format!("export_error_csv(): I/O Error ({}).", e));
        }
    }

    Result::Ok(())
}

fn test(sample1: &String, sample2: &String) {
    let track1: TrackBuffer = match import_track(sample1) {
        Ok(b)  => { b }
        Err(e) => { println!("{e}"); panic!("{e}"); }
    };

    let track2: TrackBuffer = match import_track(sample2) {
        Ok(b)  => { b }
        Err(e) => { println!("{e}"); panic!("{e}"); }
    };

    // Get and store spectograms
    println!("");
    let fft_size: u32 = 4096;
    let tracks_for_spec = vec![track1, track2];
    let spectograms: Vec<StereoSpectogram> = mt_track_to_spec(fft_size, tracks_for_spec);

    // `spectograms` has the reverse order from `tracks_for_spec`
    let _ = export_error_csv(&String::from("sepctogram2.csv"), &spectograms[0].right);
    let _ = export_error_csv(&String::from("sepctogram1.csv"), &spectograms[1].right);

    // Create time comparison
    match time_compare_spectogram(fft_size/2, &spectograms[1], &spectograms[0]) {
        Ok((c, _)) => { 
            let _ = export_error_csv(&String::from("time-comp.csv"), &c);   
        }
        Err(e) => { panic!("{e}") }
    }

    // Create frequency comparison
    match freq_compare_spectogram(fft_size/2, &spectograms[1], &spectograms[0]) {
        Ok((c, _)) => { 
            let _ = export_error_csv(&String::from("freq-comp.csv"), &c);   
        }
        Err(e) => { panic!("{e}") }
    }

}