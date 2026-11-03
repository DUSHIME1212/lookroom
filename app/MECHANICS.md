# How the original's presets work

Read from the decompiled program, version 2.27.0. Addresses are in the main
program. "Estimated" means I looked in the code and did not find the value, so
it is a choice made for this desktop version.

## The preset strip (`PresetsView`)

| What | Value | Where |
| --- | --- | --- |
| Layout | Sideways scrolling row, scroll bar hidden, grouped by category | 0x10059a618, 0x10059ac84 |
| Space under the row | 20 | 0x10059a618 |
| Panel height in preset mode | 0.2 of the editor's height, kept between 150 and 250 | 0x1001aaab4, constant 0x1011a7bf8 |
| Saved panel height | A cached height above 0 replaces the rule; this app has no resizing, so it is not used | 0x1001aad58 |
| Panel padding | 16 at the top and sides (`pad` 8, doubled) | 0x1001bd9f4, 0x1001b7608 |
| Tile side | What remains of the panel height after padding, title and the space under the row | follows from the rows above |
| Tile shape | Square (aspect ratio 1) | 0x10059b8d8 |
| Tile corner radius | 4 | 0x10059b8d8 |
| Picture share of tile height | 0.8 | 0x10059d61c (constant at 0x1011a8990) |
| Tile content | The person's own photo with that preset applied | 0x10059e074 (`sourceImageUIImage`) |
| Caption text | Caption size, semibold, black | 0x10059e2e0 |
| Caption background | White; accent colour when selected | 0x10059e2e0 |
| Caption padding | 4 on all sides | 0x10059e2e0 |
| Caption shrink limit | 0.5 of its size | 0x10059e2e0 |
| Dimmed tile opacity | 0.6 when a state flag is set; what sets it was not determined, so this app dims a tile while it is held | 0x10059d61c |
| Between groups | White line 1 wide, opacity 0.4, not after the last group | 0x10059b184 |
| Interaction | One tap applies; long press opens the menu | 0x10059b8d8 |
| Menu items | Duplicate, Rename Preset, Update Using Current | 0x10059f6ec, 0x10059ac84, app wording |
| Panel title | "Filter Presets", size 14, bold, white | 0x1001bb2d0 |
| Your presets come first | Group `0User Filters`; the 0 sorts it to the front | 0x1005ee2d0 |
| Saved in | `UserPresets/userFilter.json` | 0x1005569f0, 0x1005ee2d0 |
| Built-in catalogue | JSON files in `DefaultLuts.bundle` (Cinema, DeepLooks) | 0x100593304, 0x1005966d4 |

## Tool row

Measured from the five reference screens supplied for this version.

| What | Value |
| --- | --- |
| Tool order | Sliders, film, prism, layers, theatre masks (presets), crop, cube |
| Accent blue | #0C9FFF, sampled from the selected tab and tool |
| Panel fill | #161618; the reference panels sit around #181818 |
| Button fill | #2C2C2E |
| Layout | Photo, tool panel, tab row, tool capsule, top to bottom. From 900 points wide the panels move to a column beside the photo |
| Icons | Hugeicons free set 4.4.0, MIT licence, in `assets/hugeicons`. Film, prism and cube carry a colour run, as in the reference |

## Sliders

| What | Value | Where |
| --- | --- | --- |
| Temperature range | -3000 to 3000 | 0x10055480c |
| Temperature readout | value + 6500, shown as kelvin | 0x100553e94 |
| Contrast meaning | Range 0 to 2 at 0x10055480c, but the exact shader formula is not read | 0x10055480c |
| Tint range | -100 to 100 | 0x10055480c |
| 0 to 2 sliders (saturation and contrast use this range) | 0 to 2 | 0x10055480c |
| Saved preset fields | exposure, contrast, brightness, highlight, shadow, whites, blacks, gamma, saturation, vibrance, hue, temp, tint, fade, clarity, sharpness, vignette, grain, halation, bloom, colorGrade, luts | preset JSON in `DefaultLuts.bundle` |
| Halation defaults | threshold 0.7, glow 50 | preset JSON |
| Grain defaults | size 1, roughness 1 | preset JSON |

## Colour tables

| What | Value | Where |
| --- | --- | --- |
| Size of the built-in tables | 33 (`LUT_3D_SIZE 33`) | `.cube` files in `DefaultLuts.bundle` |
| Export sizes | 33 and 64 | app wording "Cube 33", "Cube 64" |
| Table strength | Blends original and table result | 0x10008d5a8 |
| Stacking | Several tables run in order; Cinema presets stack three | preset JSON |
| Loading | `colorCubeFilterFromLUT` in `CubeLoader.swift` | 0x10050b764 |

## Order of the colour steps

The first version of this order was read from the master render function
0x100f45c74. Steps that have been added since sit where that order puts them.
Steps marked "this app's choice" had no place in it.

1. Color Space Transform (this app's choice: it has to come first)
2. Shot Match
3. White balance and hue
4. Exposure
5. Whites and blacks
6. Highlights and shadows
7. Dehaze
8. Contrast, brightness, gamma
9. Clarity, structure
10. Film contrast, shadow toe, highlight shoulder
11. RGB curves, then the six hue and saturation curves
12. Color Wheels, then HDR Grade
13. Color grade (Tone Wheels)
14. Vibrance, saturation
15. Channel Mixer, Color Slice, HSL Mixer, Color Luminance, Tetra, Color Separation
16. Film density, film saturation
17. Film Look, Bleach Bypass, Saturate, Technicolor, Foliage, Filmic Split, Super 8 colour
18. LUTs, in stack order
19. Conversion out of DaVinci Wide Gamut, when that workflow is on
20. Diffusion
21. Bloom
22. Softness from Film Look and Super 8
23. Sharpness
24. Fade
25. Lens Distortion and Chromatic Aberration
26. Gate weave (video only, never on frame 0)
27. CRT Emulation
28. Grain
29. Halation
30. Vignette
31. Dust, flicker, Textures (this app's choice)
32. Print Simulation
33. The Super 8 gate

An exported .cube holds only steps that depend on one pixel alone: 1 to 19, Fade
and Print Simulation. Anything that looks at neighbouring pixels can't live in a LUT.

## Estimated

| What | Value here |
| --- | --- |
| Title row height | 17 (the font size is known, its line height is not stored) |
| Gap under the title | 8 (the code uses SwiftUI's default spacing) |
| Gap between tiles | 8 (the code uses SwiftUI's default spacing) |
| Inset of the group line | 16 at each end |
| Order inside colour grade | Highlights, midtones, shadows. Four bands run in a row, but they are told apart by codes I could not tie to names |
| Gamma position | Right after brightness. The render class has no gamma field |
| Vignette position | Last, before masks. The render class has no vignette field |
| Slider strengths | Exposure, contrast and saturation are standard formulas. Highlights, shadows, whites, blacks, vibrance, fade, grain, halation and bloom strengths are my own, because the originals live in compiled shaders |
| Every tool added in this version | The reference is a feature list and five pictures, so the controls follow those and all the maths and strengths are this app's own |

## Presets

Thirty built-in presets in five groups, in `src/looks.rs`.

| Group | Looks | Origin |
| --- | --- | --- |
| Cinema | Deep Cine, Tealer, Deep Cine2, Brampton, Fuji D55 to D65, Kodak D55 to D65 | The original's |
| Deep Looks | Blades, Bind, Deep V, Greenery, G58 | The original's |
| Studio | Harbor Light, Neon Rain, Long Shadow, Blue Hour, Golden Road | This app's own |
| Film | Soft Daylight, Tungsten Night, Pastel Print, Silver Skip, Three Strip | This app's own |
| Mood | Quiet Morning, Dust Bowl, Cold Iron, Ember, Plain Silver | This app's own |

The original's fifteen names and their order are from `CinemaV2.json` and
`DeepLooksV2.json` in `DefaultLuts.bundle`, and their slider and colour grade values are
the numbers saved there. They should be replaced before this app is shared. The table
below covers only those fifteen.

| What | Rule | Status |
| --- | --- | --- |
| White colour grade | Saved as neutral, so left out | From preset JSON |
| Halation | `isEnabled` false or intensity 0 in all fifteen | From preset JSON |
| Grain | `grainIntensity` 0 and no grain stock chosen in all fifteen | From preset JSON |
| Bloom | Saved `bloom` block treated as off; values look like defaults | Estimated |
| Contrast with a print table | 1 + (saved - 0.9) x 0.6, because the original's log and print chain adds contrast this app's tables do not | Estimated |
| Bind contrast | Same rule; 0.3 taken literally looks washed out | Estimated |
| Greenery contrast | Saved 0 read as not set; literally it would be solid grey | Estimated |
| Which table | Fuji uses Cool Print, Kodak uses Warm Print, at the same D number | Estimated |

The Studio, Film and Mood looks were judged with `--contact`, a sheet of those fifteen
over a landscape, a grey ramp and three skin tones. That is a generated scene and flat
patches, not real photographs.

## Curves and Tetra screens

Both follow the reference screens supplied for this version. The behaviour is this
app's own, because a picture shows the controls but not the maths.

| What | Rule |
| --- | --- |
| Mode buttons | Six, each drawn from what it reads and what it changes |
| Graph backdrop | Rainbow for hue modes, black to white for Lum, grey to colour for Sat |
| Pale line on the graph | The picture before edits: hue weighted by saturation, or light, or saturation |
| Colour dots | Place a neutral point at that hue, or select one already within 2% |
| Number boxes | Input in degrees for hue and percent otherwise. Output in degrees for Hue vs Hue, a multiplier for saturation, an offset for luminance |
| Hue vs Hue range | 120 degrees from the bottom of the graph to the top |
| Tetra leans | Red, green and blue gain another channel. Cyan, magenta and yellow lose one. Grey never moves |
| Original Image | Hold to see the picture before any edits |
| Panel handle | Drags the panel taller or shorter, in the tall layout |

The reference's Input Hue reads 69.47 for a point about halfway along, so its unit is
not degrees. What that unit is could not be worked out from a picture.

## Masks and layers

Each layer holds its own edit and is blended over the picture through its
masks, after the base edit. A layer with no masks covers the whole photo, which
is the original's Image Edit Layer (wording at 0x101404b20).

| What | Rule | Where |
| --- | --- | --- |
| Subtract | base x (1 - mask) | `maskSubtract` text, 0x101410650 |
| Intersect | base x mask | `maskIntersect` text, 0x1014105c0 |
| Add | The larger of the two | Estimated |
| Layer actions | Add mask, reset edits, duplicate, delete, show or hide, invert, combine | Field names, 0x101390ab0 |
| Brush | Distance to the stroke's line; hard centre, smooth edge; softness is the faded share of the radius | `drawMaskStroke` text, 0x101410d00 |
| Eraser | existing - amount; painting keeps the larger | `drawMaskStroke` text, 0x101410d00 |
| HSL Qualifier | Hue centre, width, softness; saturation and luminance ranges | Field names, 0x101391d70 area |
| Pale colours | Ignored by hue selection (0.02 to 0.1 of saturation is estimated) | Release note, 0x101408370 |

Three requested tools have no match in the original's strings, so they are this
app's own design: Rectangle Mask (a turned box with a feathered edge), Curve
Mask (a closed smooth curve through placed points) and Shot Match (moves the
picture's light and colour averages and spread toward a reference photo).
Linear and Radial exist there, but their exact falloff lives in compiled shaders,
so the falloff here is estimated.

## Crop

The tools follow the original's crop library and Auto Crop note (0x101409164,
0x10141b5a0): drag handles, ratios, rotate, flip and reset. Mask positions are
kept on the untouched photo, as the original's mask crop filters suggest
(`maskCropFilter`). The 45 degree straighten reach, the ratio list and the Auto
Crop scoring are estimated.

## Color Space Transform

Log curves and primaries are the makers' published figures, tested against each
format's stated middle grey where one is published.

| Input | Curve | Gamut |
| --- | --- | --- |
| Apple Log | Apple Log Profile | Rec.2020 |
| ARRI LogC3 | Exposure index 800 | ARRI Wide Gamut 3 |
| Sony S-Log3 | Technical summary | S-Gamut3.Cine |
| DJI D-Log | DJI white paper | D-Gamut |
| Fujifilm F-Log2 | Data sheet | Rec.2020 |
| RED Log3G10 | RED white paper | REDWideGamutRGB |
| DaVinci Intermediate | Blackmagic white paper | DaVinci Wide Gamut |
| Insta360 I-Log | 10-bit I-Log White Paper, see below | Rec.2020 |
| HDR HLG | ITU-R BT.2100, with grey placed per BT.2408 | Rec.2020 |
| HDR PQ | SMPTE ST 2084, with grey at 26 nits per BT.2408 | Rec.2020 |

The Filmic tone map is a fitted filmic curve, not any camera maker's own transform.

I-Log: the constants come from an open-source tool (GT-Drones/Insta360LUT) that says it
reproduces Insta360's white paper. The paper itself was not read. They do land exactly on
the paper's quoted points: black at code 93, 18% grey at 432, 2200% at 1023. That tool's
author confirmed the curve on X5 ProRes footage only, and the paper doesn't list every
camera, so other models are unconfirmed.

## Film emulation

`src/film.rs` holds a small model of each stock (dye slopes, crosstalk, base colour)
and each print. **These are this app's own numbers, chosen to behave like the stock's
character; none are measured from the real film or from the original's tables.** Stock
and print names label the character being aimed for and are not a claim of accuracy.

## Video

ffmpeg and ffprobe do the reading and writing. The app looks in the folder named by
LOOKROOM_FFMPEG, then in an `ffmpeg` folder beside itself, then beside itself, then on
the PATH. If it finds nothing, it says how to install them. They are not bundled: the two
programs come to about 430 MB, and this build is GPL, which brings source-offer duties if
it is ever redistributed with the app.

Frames travel as 16 bits a channel in both directions.

| What | Rule |
| --- | --- |
| Export | H.264 and HEVC in MP4 or MOV, ProRes 422 HQ in MOV, up to 3840 on the long side |
| Sound and tags | Copied from the source |
| Clips with no colour tag | BT.709 from 720 lines up, BT.601 below |
| HDR clips | HLG and PQ are recognised from their tags and converted automatically. Choosing another input overrides it |
| Grading nothing | Measured 55 dB luma, 42 to 45 dB chroma against a tagged ProRes source |

HDR grey cards, converted: 38% HLG signal gives 47% on screen, 58% gives 72%, 75% gives 88%.

## Motion tracking

Up to 24 textured patches are matched against a held anchor frame, with a fresh anchor
every 16 frames. Measuring from an anchor rather than the previous frame is what stops
small errors adding up. A least-squares fit gives pan, tilt, zoom and rotate, leaving
out points that disagree. Tracking writes one key per frame; keys are also how manual
moves are stored, so the two mix freely.

If too few points match, the subject is treated as hidden: the mask carries on at its
last speed and keeps looking for the same patches for up to 48 frames. Tracking runs both
ways. Video only decodes forward, so backward tracking reads 48 frames at a time.

All clips below have an exactly known move, and positions are shares of the frame.

| Clip | True | Tracked |
| --- | --- | --- |
| Pan, frame 71 | 0.3891, 0.4014 | 0.3873, 0.4014 |
| Pan, 15 frames fully hidden | 0.3891, 0.4014 | 0.3877, 0.3997 |
| Pan with heavy grain | 0.3891, 0.4014 | 0.3827, 0.3982 (also 1.4° of false turn) |
| Fast pan with motion blur, frame 47 | 0.2188, 0.2500 | 0.2181, 0.2518 |
| Pan, tracked backward to frame 0 | 0.6109, 0.5986 | 0.6128, 0.5993 |
| Turning camera | 17.75° | 17.93° |
| Centre zoom, size | 0.2284 | 0.2292 (before the anchor change's smoothing) |

These clips are generated, not filmed: a rigid picture moved by a known amount. They
stress blur, grain and blocking, but not a subject that changes shape or turns away.
No real camera footage has been tested.

## Export

| Format | Depth | Metadata |
| --- | --- | --- |
| HEIC | 8 or 10-bit | EXIF kept, read and written |
| JPEG | 8-bit | EXIF kept, read and written |
| PNG | 8 or 16-bit | EXIF kept, read and written |
| TIFF | 8 or 16-bit | EXIF kept, read and written |

`src/tiff.rs`: the image library can't read or write TIFF tags. An Exif block is itself a
small TIFF whose offsets count from its own start, so the writer keeps it at the front of
the file and puts the picture and a new main directory after it.

`src/heic.rs`: ffmpeg can encode HEVC but has no HEIC container, so the file is built here.
Above 4096 pixels on the long side a picture is split into 512-pixel tiles, as phones do.
Eight-bit pictures are labelled Main Still Picture and ten-bit ones Main 10.

Opening: HEIC and AVIF go through ffmpeg, which assembles tiles and applies the stored
rotation. Files made here with both were read back upright, down to the pixels.

8 and 10-bit, single and tiled, were all read correctly by Windows' own decoder, ffmpeg and
ImageMagick. A camera tag survived JPEG to TIFF to JPEG to HEIC to TIFF. **None of this has
been opened on an iPhone or Mac, and no real iPhone file has been read.** Those need a
device. Display P3 photos are read as if sRGB, so they look slightly less saturated.

## Timeline

The layout follows a reference picture of a video editor supplied for this version. The
behaviour is this app's own. From 1100 points wide the window is the edit workspace;
narrower, it returns to the earlier layouts, and every grading tool is on the Adjustment page.

`src/timeline.rs` holds the project and every edit, with no drawing in it. Five tracks:
video, sound, two for effects, text and stickers, and music. Each clip keeps its own full
grade, so LUTs, curves, masks and tracking all apply per clip. The editor works on the clip
under the playhead. Undo covers the whole project.

Clips of other sizes are letterboxed into the project's picture, and clips at other rates are
converted to its rate. The first clip added sets both. A transition overlaps its two clips, so
the project gets shorter by its length. A clip reads its sound from its source until Detach
Audio moves that to an item of its own.

Effects, transitions, text and stickers are this app's own designs. Text is drawn through
SVG using Segoe UI, and stickers are Hugeicons.

## Playback

Measured on this machine, with frames timed as they reached the screen. All clips are
generated test footage, in a release build.

| Clip | Grade | Before | After |
| --- | --- | --- | --- |
| 1080p 24 | None | 184 of 192 shown, worst gap 242 ms | 240 of 240, worst 53 ms |
| 1080p 24 | Film look | 6.6 fps | 24.0 fps, played at 540 px |
| 4K 30 | None | 25.9 fps, worst gap 507 ms | 30.0 fps, worst 56 ms |
| 4K 30 | Film look | 6.2 fps | 30.1 fps, played at 400 px |
| 1080p then 4K, with music | Default look | Not possible | 384 of 384, worst 64 ms |

Each clip's colour is baked to a 33-point table, so a frame costs one lookup per pixel.
Frames are decoded ahead on one thread and finished ahead on another, and sound output is
the clock. Before playing, the real grade is timed on a small picture and the largest size
that fits the frame time is chosen, from 1280 down to 400 pixels. A heavy grade therefore plays
smooth but soft. The paused picture is always full quality.

Hardware decoding was tried and dropped: on this machine it was slower for 4K. Choosing the
size once matters, because starting a decoder on a 4K clip takes about 0.8 seconds.

Playback draws from the table, so a colour can differ slightly from the paused picture.
A test holds that difference under 3%.

## Speed

Timed with `--bench CLIP` and `--ui-bench OUT`, in a release build on this machine, with a
stress project of 500 clips and 2,000 items and a 1080p clip read at 720p. Pass a file to
`--ui-bench` to time that instead of the stress project.

| Work | Before | After |
| --- | --- | --- |
| Paused: step one frame forward | 544 ms | 26 ms |
| Paused: step one frame back, footage seen | 540 ms | 19 ms |
| Paused: go back to a frame just seen | 464 ms | 16 ms |
| Paused: first step back into unseen footage | Not timed | 1017 ms |
| Paused: jump ahead into unseen footage | Not timed | 538 ms |
| App work per frame, stress project | 9.77 ms, worst 15.28 | 1.00 ms, worst 2.69 |
| Copy the project | 553 us | 0.14 us |
| Compare, nothing changed | 727 us | 0.18 us |
| Which clip is at a frame | 2.78 us | 0.02 us |
| Which items are at a frame | 2.88 us | 0.18 us |
| Build all colour tables at startup | 276 ms | 59 ms |
| Table lookup, 1 million pixels, one thread | 42.6 ms | 24.3 ms |
| Frame to screen image | 6.20 ms | 2.40 ms |
| Re-render while dragging, same grade | 87 ms | 11 ms |

What each is built from:

- `Shared` in `src/shared.rs` is copy-on-write. Clips, items, media and each clip's grade sit
  behind one, so a copy is a reference count and compares start with the pointer. Undo steps
  share everything they didn't change. Per-frame code must read before it writes: any write
  takes a private copy.
- `Index` in `src/timeline.rs` is rebuilt when the project changes. Clip starts are searched by
  bisection. Items sit sorted by start with the furthest end so far beside them, which lets a
  search stop once nothing further back can overlap.
- `Lru` in `src/shared.rs` holds decoded frames for the paused picture, up to 320 MB per clip,
  for the clip on screen and the last two. The feed reads ahead while idle and starts 16 frames
  early when going backward.
- The first look at footage still has to start ffmpeg, so the two unseen rows above stay slow.
  A keyframe index would be the next step there.
- `Lut3D::sample` is tetrahedral: four corners where trilinear read eight.
- The timeline draws only what's visible. Anything under 14 points wide is a plain bar and can
  only be grabbed near the pointer.
- While a slider or key is held the view uses a small baked table, then renders exactly once on
  release. A click that changes nothing renders nothing. Left paused, the window measured 1.4%
  of one core.

## Keyboard

`src/ui/keys.rs` holds one table that drives both the keys and the help sheet (F1, ?, or
Settings), so they can't disagree. A test checks that no two entries share a key. Nothing fires
while a text box has focus or a sheet is open. Holding a key repeats only moves, zoom and undo.
Full screen (F, the button by the clock, or a double-click on the picture) shows the picture
and its play controls alone, filling the screen. It used to make the window full screen with
the editor laid out as before, so the picture stayed small. Esc or F leaves it.
The Motion group covers keyframes (Ctrl K, Ctrl Shift K, `[`, `]`) and clip speed (R, Ctrl Up,
Ctrl Down).

## Moving text and stickers

The animation styles follow Jitter's ideas: an In, an Out and a Loop, each run across the
whole text, its lines, words or letters, with a stagger between pieces, an order (forward,
reverse or random) and an easing curve. The animations themselves are this app's own, in
`src/textanim.rs`. Nothing there draws; it says where each piece goes, and `src/overlay.rs`
draws each piece once and moves the drawing. Stickers go through the same code as a single
piece, so they get the same animations and keyframes.

The Text and Sticker pages show every animation as a tile. A tile shows its animation part of
the way through, and plays it on a loop while the pointer is over it.

Keyframes set position, scale, rotation and opacity together, each with the curve to the next
one. With no keyframes, the sliders set plain values. With keyframes, a change at the playhead
sets a keyframe there. They show as diamonds on the selected item: click one to go to it, or drag
it to move it in time. Splitting an item or trimming its start keeps every keyframe at the same
moment of the picture.

Fonts are the computer's own, plus any added with Add a Font File, which are copied into
`%APPDATA%\Lookroom\Fonts`. The list draws each name in its own font, at most three new ones a
frame, and keeps the last 160 drawn. A project names its font. On a computer without that
font, the text falls back to Segoe UI.

## Clip speed

From 0.1× to 8×, by the Speed button under the timeline, R, or Ctrl Up and Ctrl Down. A clip
keeps the same stretch of its source, so faster makes it shorter and everything after it moves
up. Pictures are retimed by ffmpeg (`setpts`, then the project's rate) and sound by `atempo`,
which keeps the pitch. Trimming, splitting, the paused picture, playback and export all read
the source through `Clip::source_at`, so they agree. A 2× read of 7 seconds of a 24 fps test clip
gave exactly 84 frames.

A project's rate comes from its first video. A clip added before that rate is known used to keep
the old rate's length: an 8 second 24 fps clip came in as 10 seconds. `Project::rate_to` now
converts every clip, item and keyframe to the new rate.

## Plugin packs

A pack is one JSON file in `%APPDATA%\Lookroom\Plugins` or a `plugins` folder beside the app.
Settings lists what is loaded and why anything wasn't, and can install a pack, reload them all, or
open the folder. Packs are data only. Nothing in one runs, numbers are kept to sensible ranges,
and a file over 2 MB is refused. `plugins/lookroom-extras.json` is built into the app and doubles
as the example:

```json
{
  "name": "My Pack", "author": "Me", "version": "1.0", "description": "Shown on hover.",
  "text_animations": [
    { "name": "Rise", "unit": "Letter", "order": "Forward", "seconds": 0.7, "delay": 0.04,
      "ease": "Back", "from": { "opacity": 0, "dy": 0.9, "rotate": -40, "scale": 1, "dx": 0, "blur": 0 } }
  ],
  "looks": [ { "name": "Night", "grade": { "temp": -900, "contrast": 1.15 } } ]
}
```

`from` is where each piece starts. It eases from there to rest, and offsets are in text heights.
Units are Whole, Line, Word or Letter. Orders are Forward, Reverse or Random. Curves are Linear,
In, Out, InOut, Back, Expo, Circ, Elastic, Bounce or Spring. A look's grade takes any of the
grade's own fields, as saved in a project. Looks appear under Plugins among the presets. A text
keeps its plugin animation's numbers, so it still plays after the pack is removed.

## Stickers, sounds and memes

Each library was picked for a licence that allows shipping it inside the app. Settings lists
every credit and licence text.

- **Stickers:** Fluent Emoji Flat by Microsoft, MIT, taken from `@iconify-json/fluent-emoji-flat`
  1.2.6. The 1,586 emoji without skin-tone variants are in `assets/stickers` as one SVG body per
  line, compiled into the app and drawn by resvg at whatever size the picture needs. The page
  has groups and search, and draws at most 12 new tiles a frame.
- **Sounds:** three Kenney packs, CC0, from OpenGameArt (kenney.nl blocks scripted downloads):
  Interface (100), Digital (62) and Foley (51). Fourteen editing staples (whoosh, swoosh, riser,
  boom, hit, pop, ding, chime, censor beep, tick, heartbeat, shutter, glitch, bass drop) are
  synthesised by `src/assets.rs`, so they need no licence. All 227 sounds are compiled in and
  written to `%APPDATA%\Lookroom\Sounds` when first heard or added. The add button puts one on
  the music track at the playhead.
- **Memes:** templates come from Imgflip's public list of its 100 most used (no key needed).
  Nothing is bundled. The list and each picture are fetched with the system's curl when the Memes
  tab asks, only from `i.imgflip.com`, and kept in `%APPDATA%\Lookroom\Memes`. A template becomes
  a photo clip with top and bottom captions in Impact, white with a black edge. The edge is a new
  Outline setting that any text can use. The pictures belong to their owners.

## Effects, transitions and previews

There are 23 effects and 23 transitions, all drawn on the processor in `src/overlay.rs`. A test
checks every transition on every pixel: exactly the outgoing picture at its start, and exactly
the incoming one at its end. Writing that test turned up a one-pixel line of the old picture
left at the end of Wipe Left and Wipe Up, now fixed.

Previews:
- Each tile shows its effect or transition at 192 × 108, on the frame being edited (and, for
  transitions, the sample scene as the second picture). At rest a tile shows the middle; while
  the pointer is over it, it plays from start to end on a loop.
- **Preview on the Picture**, or **P**, plays the picked effect's stretch, or the transition
  nearest the playhead with half a second either side, then stops there.

## Resizing

- **Panels:** drag the gap between the bin and the picture, or between the picture and the
  timeline. Double-click a gap to put it back.
- **Tracks:** drag the line under the video track's controls to make every track taller or
  shorter, from 60% to 250%. Double-click it to put it back.
- **Remembered:** sizes are kept in `settings.json`, so the window opens the way it was left.
  The picture never gets narrower than 320 points or the bin than 260.

## Design layers (Figma)

- **Bringing a design in:** in Figma, pick a frame and choose Copy as SVG, then press Ctrl+V
  in the app. Import a Figma or SVG File on the Layers tab does the same from a saved file.
- **Layers:** each top-level layer becomes a Graphic item of its own, named as in Figma, at
  its place in the frame. If the frame arrives wrapped in one plain group, the group is opened.
  A group that moves its children stays one layer, so nothing shifts.
- **Drawing:** each layer is drawn alone by resvg with `render_node`, and drawn again only
  when its size on the picture changes. Designs are kept in `%APPDATA%\Lookroom\Graphics`
  under a name taken from their text.
- **Animating:** a layer gets the same In, Out, Loop and keyframes as text. Animate Every Layer
  gives the whole design one In, each layer starting a stagger after the one below, as Jitter
  does. Their ends stay put.
- **The Layers tab:** lists everything laid over the picture at the playhead, top first: text,
  stickers, videos and design layers. Each can be shown or hidden, brought forward or sent
  back. Later in the project's list draws on top, so the order on screen matches the list.

## Video memes

There is no trending feed:
- Tenor's API shut down in June 2026.
- GIPHY's terms forbid storing copies of its media, which putting a clip in a video does.
- KLIPY doesn't publish its rules on that.

The Clips tab is a library of the person's own clips instead, kept in
`%APPDATA%\Lookroom\Video Memes`, with folders as groups. Picking one lays it over the picture
as an overlay with its own sound, size, position, animations and keyframes.

Green screens:
- **Detection:** a clip counts as green screen when at least 60% of its first frame's border
  is one green.
- **Keying:** that colour is removed by its hue relative to brightness, so shadows on the
  screen go too. The screen's spill is pulled back out of the edges.
- **Placement:** a keyed clip fills the frame. Any other clip sits in a corner, picture in
  picture.

Overlays are decoded beside the main clips, so playback, export and the paused picture all
show them. A paused frame with an overlay goes through the full compositor, which starts a
decoder, so it takes a moment longer than a plain frame.

## Linux and macOS

The code builds on all three systems. What differs lives in `src/platform.rs`:

| | Windows | macOS | Linux |
| --- | --- | --- | --- |
| The app's files | `%APPDATA%\Lookroom` | `~/Library/Application Support/Lookroom` | `$XDG_DATA_HOME/lookroom` (`~/.local/share/lookroom`) |
| Interface font | Segoe UI, read straight from its files | SF Pro or Helvetica Neue, found among installed fonts | Inter, Noto Sans, Cantarell, Ubuntu or DejaVu Sans, the first installed |
| Open Folder | Explorer | `open` | `xdg-open` |
| ffmpeg | `winget install Gyan.FFmpeg` | `brew install ffmpeg` | the distribution's package |

The differences are runtime checks rather than conditional compilation, so a Windows build
compiles every system's code path. Only the console-window flag on Windows is compiled
separately.

Packages:
- **Linux:** `packaging/build-linux.sh` makes `dist/lookroom-linux-<arch>.tar.gz`. It holds
  the program, a 512-pixel icon, a desktop entry and `install.sh`, which installs for the
  current user only (`~/.local/bin`, the menu and the icon).
- **macOS:** `packaging/build-macos.sh` makes `dist/Lookroom.app`, one universal program for
  Apple Silicon and Intel, with its icon set. It signs it for the building Mac only and zips
  it. Anyone else opens it the first time with right-click, Open, until it is signed with a
  Developer ID and notarised.
- **Icon:** `lookroom --icon OUT SIZE` writes the icon as a PNG, which is how both scripts
  get it. The window uses the same icon on every system.

`.github/workflows/build.yml` runs the tests and builds all three on GitHub's machines:
Ubuntu 22.04, macOS 14 with both Mac targets, and Windows. It uploads the packages, and a
tag such as `v0.2.0` attaches them to a release.

This machine has neither WSL nor Docker, and macOS programs can only be built on a Mac. So
only the Windows build has been run here; the Linux and macOS ones run first in that
workflow.

## The look

Reviewed with the Hallmark design skill (`~/.claude/skills/hallmark`), using its rules for
app screens, and with two reference images: a violet slider set with value bubbles, and a
dark set of tactile controls. Hallmark's rules for web pages (CSS tokens, hero, fold) don't
apply to a native app.

What changed:
- **One accent, violet (`#7B61FF`):** it marks the page's main action, a value being set and
  state. It replaced a bright blue that filled tabs, tools and rows all at once.
- **Selection is quieter.** A chosen tab or segment is lifted in a raised grey, and a chosen
  row or chip is a soft wash of the accent with a thin edge.
- **Sliders follow the reference:** a 4-pixel track, a violet fill from the neutral point,
  and a white knob ringed in violet. While a knob is pointed at or dragged, its value sits in
  a bubble above it, drawn above every panel so nothing cuts it off. Colour-band sliders
  keep their bands.
- **Focus rings:** every custom control shows one when the keyboard moves to it, drawn at
  once. They show after a Tab press and hide again after a pointer press.
- **Errors in red:** a failed download, an unreadable clip or a pack that didn't load reads
  in `ERROR` red, not hint grey.
- **Play is a white disc,** the one transport control found first.
- **Named colours:** the grey used for secondary text everywhere is now the `TEXT_2` token.

Not changed, because a restyle keeps the layout:
- the panel arrangement and the timeline's item colours;
- the multicoloured icons in the Adjustment tool row.

## What this app does not copy

The colour tables are this app's own, calculated by the recipes in
`src/looks.rs`, because permission to reuse the original's table files has not
been confirmed. Looks follow the original's settings but will not match it pixel
for pixel.
