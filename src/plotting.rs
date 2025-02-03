// Plotting (for funsies)
use plotters::prelude::*;
use std::f32::consts::PI;
use crate::types::{TrackBuffer, GraphData};

// Plots the mean deviation of each frame, over time
pub fn plot_time_error(e: &mut Vec<GraphData>, export_dir: &String) -> Result<(), String> {
    let export_path = format!("{}/error_vs_time.png", export_dir);
    let root = BitMapBackend::new(&export_path, (1000, 600)).into_drawing_area();
    
    match root.fill(&WHITE) {
        Err(_) => { return Result::Err(format!("plot_time_error():\n\tBitmapBacked::fill()")); }
        Ok(_) => {}
    }

    let mut global_max: f32 = 0.0;
    for graph in e.iter_mut() {
        if global_max < graph.get_max() {
            global_max = graph.get_max()
        }
    }

    let frame_count = e[0].data_len() as f32;
    let mut chart = match ChartBuilder::on(&root)
        .caption("Error over time", ("sans-serif", 20).into_font())
        .margin(10)
        .x_label_area_size(50)
        .y_label_area_size(50)
        .build_cartesian_2d(0f32..frame_count, 0f32..global_max*1.05) 
    {
        Ok(r)  => { r }
        Err(_) => { return Result::Err(format!("plot_time_error(): Chart creation failed.")); }
    };
    
    
    chart
        .configure_mesh()
        .disable_y_mesh()
        .y_desc("Error")
        .x_desc("Frame")
        .axis_desc_style(("sans-serif", 15))
        .draw()
        .unwrap();


    let colors_vec = vec![RED, BLUE, YELLOW, GREEN];
    let mut draw_data: Vec<(usize, f32)> = vec![];
    draw_data.reserve(frame_count as usize);

    for (i, graph) in e.iter_mut().enumerate() {

        // Create a vector with (pos, data) items for each graph as required by draw_series
        // Creating an iterator class for GraphData is more correct but won't be implemented here.
        draw_data.clear();
        loop {
            match graph.next() {
                Option::Some(t) => { draw_data.push(t); }
                Option::None    => { break; }
            }
        }

        let draw_ser = chart.draw_series(
            LineSeries::new(draw_data.iter().map(|(pos, dat): &(usize, f32)| (*pos as f32, *dat)), &colors_vec[i])
        ).unwrap()
        .label(graph.get_label());

        match i {
            0 => { draw_ser.legend(| (x,y) | PathElement::new( vec![(x,y), (x+20, y)], RED));    }
            1 => { draw_ser.legend(| (x,y) | PathElement::new( vec![(x,y), (x+20, y)], BLUE));   }
            2 => { draw_ser.legend(| (x,y) | PathElement::new( vec![(x,y), (x+20, y)], YELLOW)); }
            _ => { draw_ser.legend(| (x,y) | PathElement::new( vec![(x,y), (x+20, y)], GREEN));  }
        }
    }


    chart
        .configure_series_labels()
        .background_style(&WHITE.mix(0.8))
        .border_style(&BLACK)
        .draw()
        .unwrap();

    
    match root.present() {
        Ok(_)  => { println!("\"Error over Time\" graph ready! Exported at {}.", export_path); }
        Err(_) => { return Result::Err(format!("plot_time_error(): An error occurred before or during export.")); }
    }

    Result::Ok(())
}

// Plots the deviation of each frequency bin
pub fn plot_freq_error(e: &mut Vec<GraphData>, export_dir: &String) -> Result<(), String> {
    let export_path = format!("{}/error_by_frequency.png", export_dir);
    let root = BitMapBackend::new(&export_path, (1000, 600)).into_drawing_area();
    
    match root.fill(&WHITE) {
        Err(_) => { return Result::Err(format!("plot_freq_error():\n\tBitmapBacked::fill()")); }
        Ok(_) => {}
    }

    // Find maximum
    let mut global_max: f32 = 0.0;
    for graph in e.iter_mut() {
        if global_max < graph.get_max() {
            global_max = graph.get_max()
        }
    }

    let frame_count = e[0].data_len() as f32;
    let mut chart = match ChartBuilder::on(&root)
        .caption("Error over time", ("sans-serif", 20).into_font())
        .margin(10)
        .x_label_area_size(50)
        .y_label_area_size(50)
        .build_cartesian_2d(0f32..frame_count, 0f32..global_max*1.05) 
    {
        Ok(r)  => { r }
        Err(_) => { return Result::Err(format!("plot_time_error(): Chart creation failed.")); }
    };
    


    let mut chart = match ChartBuilder::on(&root)
        .caption("Error over time", ("sans-serif", 20).into_font())
        .margin(15)
        .x_label_area_size(30)
        .y_label_area_size(30)
        .build_cartesian_2d((0u32..e.len() as u32).into_segmented(), 0f32..max_y*1.05) 
    {
        Ok(r)  => { r }
        Err(_) => { return Result::Err(format!("plot_time_error(): Chart creation failed.")); }
    };
    
    
    chart
        .configure_mesh()
        .disable_x_mesh()
        .bold_line_style(WHITE.mix(0.3))
        .y_desc("Error")
        .x_desc("Frame")
        .axis_desc_style(("sans-serif", 15))
        .draw()
        .unwrap();



    chart.draw_series(
        Histogram::vertical(&chart)
            .style(RED.mix(0.5).filled())
            .data(data.iter().map(|(x, y): &(u32, f32)| (*x, *y)))
    ).unwrap();

    root.present().unwrap();

    Result::Ok(())
}

// Plots a spectogram, yooohoo
pub fn plot_spectogram(frames: &TrackBuffer, bins: u32, filename: &String) -> Result<(), String> {
    if frames.len() % bins as usize != 0 {
        return Result::Err(format!("plot_spectogram(): The number of bins ({}) doesn't match the size of the input vector ({} / {} = {})", bins, frames.len(), bins, frames.len() as f32 / bins as f32));
    }
    if frames.len() > u32::MAX as usize{
        return Result::Err(format!("plot_spectogram(): The number of input frames is too large. Aborting."));
    }
    let frame_count: u32 = frames.len() as u32 / bins;
    let root = BitMapBackend::new(filename, (frame_count*4, bins)).into_drawing_area();

    match root.fill(&BLACK) {
        Ok(_) => {}
        Err(_) => { return Result::Err(format!("plot_spectogram():\n\tBitmapBacked::fill() ")); }
    }

    let mut chart = match ChartBuilder::on(&root) 
        .margin(20)
        .x_label_area_size(10)
        .y_label_area_size(10)
        .build_cartesian_2d(0..frame_count*4, 0..bins) {

        Ok(c) => { c }
        Err(_) => { return Result::Err(format!("plot_spectogram(): Chart creation failed.")); }
    };

    match chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .draw() 
    {
        Ok(_)  => {}
        Err(_) => { return Result::Err(format!("plot_spectogram():\n\tChartBuilder::draw() ")); }
    }

    let plotting_area = chart.plotting_area();

    let mut frame_it = frames.iter();

    for x in 0..frame_count {
        print!("\rPlotting... {}%", x*100/frame_count);
        
        for y in 0..bins {
            let mut bin_val: f32 = *frame_it.next().unwrap();
            if bin_val < 0.3 { continue; }
            
            bin_val /= (bins as f32).sqrt();
            bin_val = ( bin_val *PI / 2.0).cos();

            for e in 0..4 {
                match plotting_area.draw_pixel((x*4+e, y), &MandelbrotHSL::get_color(bin_val)) {
                    Ok(_) => {}
                    Err(_) => { println!("plot_spectogram: draw_pixel failed!"); }
                }
            }
        }
    }

    match root.present() {
        Ok(_)  => { println!("\rSpectogram ready! Saved at {}", filename); Result::Ok(()) }
        Err(_) => { Result::Err(format!("plot_spectogram(): Could not save image.")) }
    }

}