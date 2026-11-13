# Long-video engineering review

These findings come from source inspection. They are failure mechanisms, not a confirmed diagnosis of a particular user's video; no failing media or crash log was provided.

## Corrected in this change

- `video::strip` previously scanned all keyframes and collected raw RGB thumbnails with `Command::output` before choosing the requested samples. Duration and keyframe density increased both work and peak memory. It now seeks to a fixed number of evenly distributed timestamps and requests exactly one frame per seek. This bounds output memory; seeking can still be expensive with long GOPs or poorly indexed media.
- The paused-frame cache imposed a 16-frame minimum, overriding its 320 MiB budget. A 3840×2160 RGB float frame occupies approximately 95 MiB; 16 such frames occupy approximately 1.48 GiB per decoder before other buffers. Cache capacity now follows the budget with a one-frame floor. This budget is per feed, not a global process limit.
- Both export paths wrote frames while leaving FFmpeg's stderr pipe unread until the end. Enough diagnostics could fill the pipe, block FFmpeg, then block the frame writer. A concurrent reader now drains diagnostics and keeps only the last 64 KiB.

- Playback preview resolution was selected once from grading-only timings. It now measures actual decode/composition/scope/image work and steps down resolution after 30 over-budget samples when behind the playback clock. This reduces sustained grading/composition pressure, but does not replace proxies for slow source decoding. Short timelines also preroll only the number of frames available.

## Remaining architectural constraints

- `Reader::next` expands decoded pixels into three 32-bit floats and CPU processing handles grading/composition. Full-resolution 4K export has substantial memory bandwidth and CPU cost per frame. Longer duration multiplies that cost; duration alone does not imply all frames are held in memory.
- `audio::Shared::fill` advances the playback clock only for samples actually consumed. Audio underruns therefore stall the video clock as well. This preserves sample alignment but can look like a frozen picture; underrun telemetry and a deliberate rebuffer policy are needed before changing clock semantics.
- Preview caches and spare decoders have separate budgets. Multiple media sources can still raise process memory significantly. A global byte budget, decoder eviction, and disk-backed low-resolution proxies would make resource use more predictable.
- Timing primarily uses frame indices and an average frame rate. Variable-frame-rate phone recordings need a timestamp-aware ingest/proxy strategy to guarantee accurate seeking and sustained audio/video synchronization.
- Decoder stderr is discarded and a failed frame read can look like end-of-file. Export should distinguish clean completion from truncated decoding, preserve decoder diagnostics, and validate the expected output frame count.
- Thumbnail jobs have a fixed worker pool but no project generation/cancellation token. Opening another project can leave obsolete work running. Job cancellation and generation-tagged results should accompany project switching.

## Recommended order

1. Reproduce with the failing file and capture OS memory usage, decoder/encoder diagnostics, export settings, and the failing timestamp.
2. Add decoder failure propagation and output frame-count validation.
3. Introduce timestamp-normalized proxies, a global memory budget, and cancellation across media jobs.
4. Profile representative 60–120 minute projects before choosing GPU grading/composition work. Hardware encoding alone does not remove CPU grading cost.

## Validation scope

The full release-mode Cargo check passed without warnings. Two focused tests compiled directly from the resource helper implementations passed: the 4K frame-cache budget and draining a 200 KB diagnostics stream while retaining 64 KiB. Regression cases are also included in the application test module; the full application test suite has not been run. The home screen has not been visually verified. A real long-video playback/export soak test remains necessary to establish stability on the target machine.
