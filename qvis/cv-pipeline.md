UI:
- The user picks out which pixels correspond to which stickers using floodfilling

Calibration:
- Take a bunch of pictures and save them
- For each pixel and for each sticker it could be, run an ANOVA test on the calibrated data to see if it matches the sticker
  - Refine the assignment as more calibrations come in; adjust α down in a sensible way
  - Display the assignment to the user
  - Once the user is happy with the assignment, stop this step
- For each pixel, construct a quadtree or similar that would allow KNN to be done
- Optional: Some kind of optimization routine that picks the colorspace that separates the colors the most

Prediction:
- For each sticker
  - For each pixel assigned to it (or a random subset of pixels if too slow)
    - Optional: White balance
    - For each possible color
      - Perform a KNN search to guesstimate the density of observations in that area
      - Take that to be the confidence of that color being correct
  - For each possible color
    - Order the pixels by confidence in that color and pick the 80th percentile in terms of confidence (or some other number) as the confidence of this color in this pixel
      - (If the percentile is too low then we may miss some pixels that are partially unshadowed but if it's too high we might accidentally use outliers)
- Input to matching algo
