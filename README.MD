# Spectogram Comparison Utility
Utility for comparing results of X-UMX (audio separation) in parallel, written in Rust.

This program is intended to be used as a metric tool for X-UMX inference results, by calculating the mean error between STFT frames of each output tracks pair (e.g. bass output of dir1 with bass output for dir2). The program displays the error found on each pair of stems (Bass, Drums, Vocals and Other) as well as the mean error of the 4. The program uses 2 algorithms for calculating the final errors, one where all frequency bins of respective STFT frames are treated equally (Time mode) and one where bins of higher frequency have a smaller effect on the overall results (Frequency mode). The latter method was implemented so that the final metric would loosely reflect the Frequency/Perceived Loudness curve of the human ear. 

The metric itself doesn't have a quantifiable meaning but can be used to measure relative changes in quality between different settings in X-UMX. 

## Usage
The program expects two directories as input arguments, where each directory contains the separated stems of a song.

```
speccomp directory1 directory2 [--serial]
```

The `--serial` flag is optional and will force the program to execute in one thread instead of 8. This option is available for testing purposes.

## Multithreaded Execution
Considering this task requires independant computations on 8 distinct tracks for the calculations of 8 different spectograms, utilizing multithreading allowed for speedups around 2.0 - 3.0.

| **Input Duration** | **Serial Exec. Time** | **Parallel Exec. Time** | **Speed-Up** |
|-----------|-------------|------------|-------------|
| 44 s | 306 ms | 142 ms | 2.15 |
| 1 min | 433 ms | 163 ms | 2.65 |
| 4 min | 1594 ms | 509 ms | 3.13 |

The parallel version is by no means fully optimized. The final portion of the code, where the deviations between frequency bins are calculated, is not implemented to run in parallel yet.


