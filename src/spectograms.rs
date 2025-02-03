use crate::types::*;

use std::{
    time::{Duration, Instant}, f32::consts::PI,
    thread::JoinHandle, sync::{Arc, Mutex}, cmp::min,
    thread, sync::mpsc::{Sender, Receiver, channel}
};

// FFT algorithms for STFT
use rustfft::{FftPlanner, num_complex::Complex};


// Multithreaded variants ---------------------------------------------------------------------------------------------------
// Calculates the spectogram of each track in `input_tracks` in parallel.
// THe returned spectograms are stored in the reverse order from which their inputs were given.
// `input_tracks` is consumed (no need to go the extra mile so that it doesn't.)
pub fn mt_track_to_spec(fft_size_u32: u32, input_tracks: Vec<TrackBuffer>) -> Vec<StereoSpectogram> {
    let fft_size: usize = fft_size_u32 as usize;
    let input_count: usize = input_tracks.len();

    // Use n MPSC pairs, one for each input track
    let mut receivers: Vec<Receiver<i32>> = vec![];
    receivers.reserve(input_count);

    // Create the shared vectors each thread will use
    let mut shared_buffers: Vec<Arc<Mutex<StereoSpectogram>>> = vec![];
    shared_buffers.reserve(input_count);

    // Spawn threads
    let mut handles: Vec<JoinHandle<_>> = vec![];
    handles.reserve(input_count);

    // Start threads
    for input in input_tracks {
        let new_buffer: Arc<Mutex<StereoSpectogram>> = Arc::new(Mutex::<StereoSpectogram>::new(StereoSpectogram::new()));
        shared_buffers.push(new_buffer.clone());

        let (tx, rx) = channel();
        receivers.push(rx);

        handles.push(
            thread::spawn(move || mt_track_to_spec_thread(fft_size, &input, tx, new_buffer.clone()))
        );
    }

    // Wait for threads and poll their progress
    let mut thread_progress: Vec<i32> = vec![];
    thread_progress.resize(input_count, 0);

    let mut threads_finished: usize = 0;
    let mut overall_progress: usize;
    while threads_finished < input_count {
        threads_finished = 0;
        overall_progress = 0;

        for i in 0..input_count {
            // Check if the thread finished
            if handles[i].is_finished() { 
                threads_finished += 1;
                continue;
            }

            // Read thread's channel
            match receivers[i].recv() {
                Ok(r) => {
                    thread_progress[i] = r;
                    overall_progress += r as usize
                }
                Err(_) => {
                    println!("\nWarning: Thread {} is not responding.", i);
                    thread_progress[i] = -1;
                }
            }
        }
            // Print state
        print!("\r Calculating spectograms ({}%)... [ ", overall_progress/input_count);
        for i in 0..input_count {
            if thread_progress[i] == -1 { print!(" ERR \t"); }
            else if thread_progress[i] >= 100 { print!("DONE \t"); }
            else { print!("0{}% \t", thread_progress[i]); }
        }
        print!("]");

        // Sleep
        thread::sleep(Duration::from_millis(1));
    }
    print!("\rAll spectograms are ready.                                         \n");

    // Return the shared buffers
    let mut spectograms: Vec<StereoSpectogram> = vec![];
    spectograms.reserve(input_count);
    for _ in 0..input_count {
        // Take the arc out of the vector first
        let popped_arc = shared_buffers.pop().unwrap();
        // Extract mutex from arc
        let spec = Arc::try_unwrap(popped_arc).unwrap();
        // Extract spectogram from mutex
        let spec = spec.into_inner().unwrap();
        spectograms.push(spec);
    }

    return spectograms;
}

fn mt_track_to_spec_thread(fft_size: usize, input_track: &TrackBuffer, tx: Sender<i32>, output_buffer: Arc<Mutex<StereoSpectogram>>) {
    // Number of samples and number of samples per channel
    let buffer_size: usize = input_track.len();
    let buffer_duration: usize = buffer_size / 2;

    // Lock the return buffer
    let mut return_buffer = output_buffer.lock().unwrap();

    // Create a Hann window
    let a0 :f32 = 0.5;
    let a1: f32 = 1f32 - a0;

    let window_size: f32 = fft_size as f32;
    let window_edge: i64 = fft_size as i64 / 2;

    let mut hann_window: Vec<f32> = vec![];
    for n in -1*window_edge..window_edge {
        let n_f32: f32 = n as f32;

        let temp: f32 = 2f32*PI*n_f32 / window_size;
        let w_n: f32 = a0 - a1 * temp.cos();
        hann_window.push(w_n);
    }

    // Create rustfft::fft object
    let mut fft_planner: FftPlanner<f32> = FftPlanner::new();
    let fft = fft_planner.plan_fft_forward(fft_size);

    // We'll create `fft_size` length windows until `input_track` has been iterated to its entirety
    let mut samples_processed: usize = 0;
    let source = input_track.as_slice();

    let mut window_buffer_l: Vec<Complex<f32>> = vec![];
    window_buffer_l.reserve(fft_size);

    let mut window_buffer_r: Vec<Complex<f32>> = vec![];
    window_buffer_r.reserve(fft_size);

    // Buffers to store the result spectograms
    return_buffer.left.reserve(fft_size * buffer_duration/fft_size); // yes, this is redundant but conveys that this buffer isn't about samples
    return_buffer.right.reserve(fft_size * buffer_duration/fft_size);

    let mut last_percentage: i32 = 0;
    let mut new_percentage: i32;
    loop {

        new_percentage = (samples_processed * 100 / buffer_duration) as i32;
        if new_percentage > last_percentage {
            let _ = tx.send(new_percentage);
            last_percentage = new_percentage;
        }

        // Check if this window will exceed the input buffer's size
        match samples_processed + fft_size > buffer_duration {
            false => { // No need to pad
                for i in 0..fft_size {
                    let idx = 2*(i + samples_processed);
                    window_buffer_l.push(Complex::new(source[idx] * hann_window[i],   0.0f32));
                    window_buffer_r.push(Complex::new(source[idx+1] * hann_window[i], 0.0f32));
                }
            }

            true => { // Will have to pad
                // Get all remaining samples
                for i in 0..(buffer_duration % fft_size) {
                    let idx = 2*(i + samples_processed);
                    window_buffer_l.push(Complex::new(source[idx] * hann_window[i],   0.0f32));
                    window_buffer_r.push(Complex::new(source[idx+1] * hann_window[i], 0.0f32));
                }
                // Pad with 0
                for _i in (buffer_duration%fft_size)..fft_size {
                    window_buffer_l.push(Complex::new(0f32, 0f32));
                    window_buffer_r.push(Complex::new(0f32, 0f32));
                }
            }
        }
        
        // Perform the FFT operation
        fft.process(&mut window_buffer_l); // process() returns the output within the input argument
        fft.process(&mut window_buffer_r);
        

        // Calculate the spectogram
        for i in 0..fft_size/2 {
            return_buffer.left.push(window_buffer_l[i].re.powi(2));
            return_buffer.right.push(window_buffer_r[i].re.powi(2));
        }

        // Reset input/processing buffer; no need to re-allocate
        window_buffer_l.clear();
        window_buffer_r.clear();

        samples_processed += fft_size;
        if samples_processed > buffer_duration { break; }
    }

    let _ = tx.send(100);

    // The mutex will be unlocked automatically now
}


// Single core variants -----------------------------------------------------------------------------------------------------
// Convert a track to a spectogram
pub fn track_to_spec(fft_size_u32: u32, sample_buffer: &TrackBuffer) -> StereoSpectogram {
    let fft_size: usize = fft_size_u32 as usize;

    // Number of samples and number of samples per channel
    let buffer_size: usize = sample_buffer.len();
    let buffer_duration: usize = buffer_size / 2;

    // Create a Hann window
    let a0 :f32 = 0.5;
    let a1: f32 = 1f32 - a0;

    let window_size: f32 = fft_size as f32;
    let window_edge: i64 = fft_size as i64 / 2;

    let mut hann_window: Vec<f32> = vec![];
    for n in -1*window_edge..window_edge {
        let n_f32: f32 = n as f32;

        let temp: f32 = 2f32*PI*n_f32 / window_size;
        let w_n: f32 = a0 - a1 * temp.cos();
        hann_window.push(w_n);
    }

    // Create rustfft::fft object
    let mut fft_planner: FftPlanner<f32> = FftPlanner::new();
    let fft = fft_planner.plan_fft_forward(fft_size);

    // We'll create `fft_size` length windows until `sample_buffer` has been iterated to its entirety
    let mut window_buffer_l: Vec<Complex<f32>> = vec![];
    window_buffer_l.reserve(fft_size);

    let mut window_buffer_r: Vec<Complex<f32>> = vec![];
    window_buffer_r.reserve(fft_size);

    // Buffers to store the result spectograms
    let mut spectogram_buffer_l: Vec<f32> = vec![];
    spectogram_buffer_l.reserve(fft_size * buffer_duration/fft_size); // yes, this is redundant but conveys that this buffer isn't about samples

    let mut spectogram_buffer_r: Vec<f32> = vec![];
    spectogram_buffer_r.reserve(fft_size * buffer_duration/fft_size);

    let mut samples_processed: usize = 0;
    let source = sample_buffer.as_slice();

    let mut frames_generated: u32 = 0;
    let processing_start = Instant::now();
    loop {
        if samples_processed % 128 == 0 {
            //print!("\r Generating spectogram... {}%", samples_processed*100/buffer_duration);
        }

        // Check if this window will exceed the input buffer's size
        match samples_processed + fft_size > buffer_duration {
            false => { // No need to pad
                for i in 0..fft_size {
                    let idx = 2*(i + samples_processed);
                    window_buffer_l.push(Complex::new(source[idx] * hann_window[i],   0.0f32));
                    window_buffer_r.push(Complex::new(source[idx+1] * hann_window[i], 0.0f32));
                }
            }

            true => { // Will have to pad
                // Get all remaining samples
                for i in 0..(buffer_duration % fft_size) {
                    let idx = 2*(i + samples_processed);
                    window_buffer_l.push(Complex::new(source[idx] * hann_window[i],   0.0f32));
                    window_buffer_r.push(Complex::new(source[idx+1] * hann_window[i], 0.0f32));
                }
                // Pad with 0
                for _i in (buffer_duration%fft_size)..fft_size {
                    window_buffer_l.push(Complex::new(0f32, 0f32));
                    window_buffer_r.push(Complex::new(0f32, 0f32));
                }
            }
        }
        
        // Perform the FFT operation
        fft.process(&mut window_buffer_l); // process() returns the output within the input argument
        fft.process(&mut window_buffer_r);
        

        // Calculate the spectogram
        for i in 0..fft_size/2 {
            spectogram_buffer_l.push(window_buffer_l[i].re.powi(2));
            spectogram_buffer_r.push(window_buffer_r[i].re.powi(2));
        }

        // Reset input/processing buffer; no need to re-allocate
        window_buffer_l.clear();
        window_buffer_r.clear();

        samples_processed += fft_size;
        frames_generated += 1;
        if samples_processed > buffer_duration { break; }
    }

    let processing_time = processing_start.elapsed();
    println!("\r        Generated {} spectogram frames ({} bins each).\t[{} ms]", frames_generated, fft_size/2, processing_time.as_millis());

    // Return sepctograms
    StereoSpectogram {left: spectogram_buffer_l, right: spectogram_buffer_r}
}



// Compares two stereo spectograms; Returns a tuple: a vector with the mean error of each frame and the total mean error
// The error of each channel is calculated independantly and the mean of the two is kept
pub fn time_compare_spectogram(bins: u32, spec_a: &StereoSpectogram, spec_b: &StereoSpectogram) -> Result<(Vec<f32>, f32), String> {
    let bins_us = bins as usize;

    let (spec_a_l, spec_a_r) = (&spec_a.left, &spec_a.right);
    let (spec_b_l, spec_b_r) = (&spec_b.left, &spec_b.right);

    // Find frame count
    let spec_a_frame_count = spec_a_l.len() / bins_us;
    let spec_b_frame_count = spec_b_l.len() / bins_us;
    let usable_frames = min(spec_a_frame_count, spec_b_frame_count);

    // Check the numbers add up
    if spec_a_l.len() % bins_us != 0 { 
        return Result::Err(format!("time_compare_spectogram(): The number of bins in input a ({}) doesn't match the size of the input vector ({} / {} = {})",
            bins, spec_a_l.len(), bins, spec_a_l.len() as f32 / bins as f32));
    }

    if spec_b_l.len() % bins_us != 0 { 
        return Result::Err(format!("time_compare_spectogram(): The number of bins in input b ({}) doesn't match the size of the input vector ({} / {} = {})",
            bins, spec_b_l.len(), bins, spec_b_l.len() as f32 / bins as f32));
    }

    // Warn user if a frame count mismatch occurred
    if spec_a_frame_count != spec_b_frame_count { 
        println!("Warning: Different input sizes (spec_a: {} frames, spec_b: {} frames), using {} frames.", 
        spec_a_frame_count, spec_b_frame_count, usable_frames); 
    }

    let mut mean_err_vec: Vec<f32> = vec![];
    mean_err_vec.reserve(usable_frames as usize);

    let mut mean_error: f32 = 0.0;
    let mut a_it_l = spec_a_l.iter();
    let mut b_it_l = spec_b_l.iter();
    let mut a_it_r = spec_a_r.iter();
    let mut b_it_r = spec_b_r.iter();
    let mut a_st: f32;
    let mut b_st: f32;
    for f in 0..usable_frames {
        if f % 16 == 0 { print!("\rComparing... {}%", f*100/usable_frames); }

        let mut frame_error: f32 = 0.0;
        for _ in 0..bins {
            a_st = (a_it_l.next().unwrap() + a_it_r.next().unwrap()) / 2.0;
            b_st = (b_it_l.next().unwrap() + b_it_r.next().unwrap()) / 2.0;
            frame_error += (a_st - b_st).abs();
        }
        frame_error /= bins as f32;

        // Store error
        mean_err_vec.push(frame_error);

        // Find mean error for a quick check
        mean_error += frame_error;
    }
    mean_error /= usable_frames as f32;

    Result::Ok((mean_err_vec, mean_error))
}

// Compares two stereo spectograms in terms of frequency; For each bin, the mean error from all frames is returned
// If `has_original` is set, `spec_a` is treated as the original.
pub fn freq_compare_spectogram(bins: u32, spec_a: &StereoSpectogram, spec_b: &StereoSpectogram) -> Result<(Vec<f32>, f32), String> {
    let bins_us = bins as usize;

    let (spec_a_l, spec_a_r) = (&spec_a.left, &spec_a.right);
    let (spec_b_l, spec_b_r) = (&spec_b.left, &spec_b.right);

    // Find frame count
    let spec_a_frame_count = spec_a_l.len() / bins_us;
    let spec_b_frame_count = spec_b_l.len() / bins_us;
    let usable_frames = min(spec_a_frame_count, spec_b_frame_count);

    // Check the numbers add up
    if spec_a_l.len() % bins_us != 0 { 
        return Result::Err(format!("time_compare_spectogram(): The number of bins in input a ({}) doesn't match the size of the input vector ({} / {} = {})",
            bins, spec_a_l.len(), bins, spec_a_l.len() as f32 / bins as f32));
    }

    if spec_b_l.len() % bins_us != 0 { 
        return Result::Err(format!("time_compare_spectogram(): The number of bins in input b ({}) doesn't match the size of the input vector ({} / {} = {})",
            bins, spec_b_l.len(), bins, spec_b_l.len() as f32 / bins as f32));
    }

    // Warn user if a frame count mismatch occurred
    if spec_a_frame_count != spec_b_frame_count { 
        println!("Warning: Inputs of compare_spectogram have different sizes (spec_a: {} frames, spec_b: {} frames), only {} frames will be used.", 
            spec_a_frame_count, spec_b_frame_count, usable_frames); 
    }

    // Create weights
    let mut w: Vec<f32> = vec![];
    w.resize(bins as usize, 1.0);
    // Audio information above 4KHz is less usefull; 4KHz ~= bin 371
    for i in 0..bins as usize {
        let i_f = i as f32;
        w[i] = 1.0 - ((i_f * PI + 370.0)/(bins as f32)).cos();
        w[i] = 1.0 - w[i].powi(2) / 4.0;
    }

    // Iteration through the vectors still happens from bin to bin in each frame; allocate all result bins now
    let mut mean_err_vec: Vec<f32> = vec![];
    mean_err_vec.resize(bins as usize, 0.0);

    let mut a_it_l = spec_a_l.iter();
    let mut b_it_l = spec_b_l.iter();
    let mut a_it_r = spec_a_r.iter();
    let mut b_it_r = spec_b_r.iter();
    let mut a_st: f32;
    let mut b_st: f32;
    for f in 0..usable_frames {
        if f % 16 == 0 { print!("\rComparing... {}%", f*100/usable_frames); }

        for bin in 0..bins {
            a_st = (a_it_l.next().unwrap() + a_it_r.next().unwrap()) / 2.0;
            b_st = (b_it_l.next().unwrap() + b_it_r.next().unwrap()) / 2.0;
            mean_err_vec[bin as usize] += (a_st - b_st).abs() * w[bin as usize];
        }  
    }

    // Divide each bin's error sum error to get the mean
    let mut mean_error: f32 = 0.0;
    for b in 0..bins {
        mean_err_vec[b as usize] /= usable_frames as f32;
        mean_error += mean_err_vec[b as usize];
    }
    mean_error /= bins as f32;

    Result::Ok((mean_err_vec, mean_error))
}
