#[derive(Debug)]
pub struct StereoSpectogram {
    pub left: Vec<f32>,
    pub right: Vec<f32>
}

impl StereoSpectogram {
    pub fn new() -> StereoSpectogram {
        StereoSpectogram { left:vec![], right:vec![] }
    }
}

pub struct GraphData {
    data:   Vec<f32>,
    label:  String,
    max:    Option<f32>,

    it_pos: usize
}

impl GraphData {
    pub fn new(v: Vec<f32>, label: String) -> GraphData {
        GraphData {
            data: v,
            label: label,
            max:    Option::None,

            // For the iterator
            it_pos: 0
        }
    }

    pub fn get_max(&mut self) -> f32 {
        match self.max {
            Option::Some(m) => m,

            // The max value has not been found yet
            Option::None => {
                let mut self_it = self.data.iter();

                let mut temp_max: f32 = 0.0;
                loop {
                    match self_it.next () {
                        Option::Some(f) => { temp_max = if *f >temp_max {*f} else {temp_max}; }
                        Option::None => { break; }
                    }
                }

                self.max = Option::Some(temp_max);
                temp_max
            }
        }
    }

    pub fn data_len(&self) -> usize {
        self.data.len()
    }

    pub fn get_label(&self) -> &String {
        &self.label
    }

}

impl Iterator for GraphData {
    type Item = (usize, f32);

    fn next(&mut self) -> Option<(usize, f32)> {
        match self.it_pos < self.data.len() {
            true => {
                self.it_pos += 1;
                Option::Some((self.it_pos-1, self.data[self.it_pos-1]))
            }

            false => { Option::None }
        }
    }
}

// Type for storing tracks with the channels interleaved (e.g. [L0, R0, L1, R1, ...])
pub type TrackBuffer = Vec<f32>;