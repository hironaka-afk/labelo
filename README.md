# Simple annotation tool for image sequences
A simple tool for drawing boxes around stuff in videos, and label them with some attribute (e.g. label animals in a short video clip). Only supports boxes, nothing fancy.

# Usage
Run it:
```sh
$> labelo -h
Labeling video sequences. The sequences should be images in a directory, named so that they can be sorted alphanumerically. Run without the --label_config argument to generate a default label configuration file in ~/.labelo_config.toml. You can use that to build your own. If the file defined by --label_config does not exist, it will also be created and filled with default values, so you can modify it to your liking

Usage: labelo [OPTIONS]

Options:
  -l, --label-config <LABEL_CONFIG>  Label configuration file, defining which labels to use
  -i, --input-dir <INPUT_DIR>        Input directory containing the images
  -o, --output-file <OUTPUT_FILE>    Output label file (json format). If the file exists, it will be read at startup [default: labels.json]
  -h, --help                         Print help
  -V, --version                      Print version
```

E.g. `labelo -l labelo_config.toml -i my_images_dir/ -o my_labels.json`

# Config file
Looks like this:
```toml
[[label_configs]]

[label_configs.S]
name = "animal"
states = [
    "cat",
    "dog",
    "possum",
]
optional = false

[[label_configs]]

[label_configs.I]
name = "size"
first = 1
last = 10
optional = true
```
You can add entries of type `[label_configs.I]` for integers in some range, or `[label_configs.S]` for a string, as many as you like. The `optional` entry is not used.

# Input directory
The input directory contains the images as png or jpeg. They must be numbered or somehow named so they can be brought in alphanumeric order. You can use some tool like `ffmpeg` to extract images from videos.

# Output file
The output is a json file that contains the labels. A label is a sequence of boxes with some label information defined in the config file. Label sequences are stored per frame, except when they are marked as invisible in the GUI. The label can go over many frames, and there can be many labels in the output file. Try it out and look at the output file.
The boxes in the output file are normalized to [0,1], not in pixels.

# How to use
Start the program:
`labelo -l labelo_config.toml -i my_images_dir/ -o my_labels.json`

# UI
[./labelo_ui.png](./labelo_ui.png)

There are some tools on the left side, and the images on the right side. Scroll through the images with the slider, the left/right buttons, or the left/right arrow keys. The Play button will play the images as fast as it can, frame rate is not guaranteed.

- Add sequence: Add a new sequence of boxes for a new object
- Save annotations: Save the annotations to json file given on command line

To label some object:
- Add sequence
- Draw a box around the object where it first appears
- Select labels (e.g. "bird")
- Scroll through images and adjust the box so it follows the object
- When the object is no longer visible, check "invisible" in the tools

Every change in the box will add a new keyframe, in between the boxes will be interpolated.

- When done, press Ctrl+Q (will save annotations and quit)

[./labelo_example.png](./labelo_example.png)

# Output
Looks like this:
```json
[
  {
    "annotations": [
      {
        "labels": [
          {
            "S": {
              "name": "animal",
              "state": "bird"
            }
          },
          {
            "I": {
              "name": "size",
              "state": 1
            }
          }
        ],
        "bbox": {
          "mins": {
            "x": 0.66599023,
            "y": 0.5412418
          },
          "maxs": {
            "x": 0.67565876,
            "y": 0.5661953
          }
        },
        "frame": 138,
        "invisible": false,
        "interpolated": false
      },
```
and so on.