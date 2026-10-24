param([Parameter(Mandatory)][string]$Evidence, [int]$Frame = 95)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$columns = @(
    @{ Title = '64-sample reference'; File = 'aa-{0:D4}-reference.png' -f $Frame },
    @{ Title = 'Native / no AA'; File = 'aa-{0:D4}-native-no-aa.png' -f $Frame },
    @{ Title = 'XeSS Native AA'; File = 'aa-{0:D4}-xess.png' -f $Frame },
    @{ Title = 'XeSS Quality'; File = 'quality-{0:D4}-xess.png' -f $Frame },
    @{ Title = 'XeSS Balanced'; File = 'balanced-{0:D4}-xess.png' -f $Frame }
)
$regions = @(
    @{ Name = 'Oblique wires'; X = 0.17; Y = 0.24 },
    @{ Name = 'Fine checkerboard'; X = 0.50; Y = 0.24 },
    @{ Name = 'Curved edges'; X = 0.82; Y = 0.24 },
    @{ Name = 'Moving foreground'; X = 0.65; Y = 0.62 }
)
$sheet = [Drawing.Bitmap]::new(1620, 966)
$graphics = [Drawing.Graphics]::FromImage($sheet)
$font = [Drawing.Font]::new('Segoe UI', 12)
$small = [Drawing.Font]::new('Segoe UI', 10)
$brush = [Drawing.SolidBrush]::new([Drawing.Color]::FromArgb(225, 231, 240))
try {
    $graphics.Clear([Drawing.Color]::FromArgb(20, 25, 34))
    $graphics.DrawString("XeSS Vulkan | $([IO.Path]::GetFileName($Evidence)) | frame $Frame | crops enlarged 2x", $font, $brush, 10, 8)
    $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::NearestNeighbor
    $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::Half
    for ($col = 0; $col -lt $columns.Count; $col++) {
        $image = [Drawing.Bitmap]::new((Join-Path $Evidence $columns[$col].File))
        try {
            $graphics.DrawString($columns[$col].Title, $font, $brush, (10 + 322 * $col), 40)
            for ($row = 0; $row -lt $regions.Count; $row++) {
                $region = $regions[$row]
                $x = [Math]::Min($image.Width - 160, [Math]::Max(0, [int]($image.Width * $region.X) - 80))
                $y = [Math]::Min($image.Height - 90, [Math]::Max(0, [int]($image.Height * $region.Y) - 45))
                $top = 72 + $row * 216
                $graphics.DrawString($region.Name, $small, $brush, (10 + 322 * $col), $top)
                $graphics.DrawImage($image, [Drawing.Rectangle]::new((10 + 322 * $col), ($top + 23), 320, 180), $x, $y, 160, 90, [Drawing.GraphicsUnit]::Pixel)
            }
        } finally { $image.Dispose() }
    }
    $graphics.DrawString('Analytic scene; linear input/output; sRGB PNG previews. This does not establish game quality or FPS.', $small, $brush, 10, 942)
    $destination = Join-Path $Evidence ('comparison-{0:D4}.png' -f $Frame)
    $sheet.Save($destination, [Drawing.Imaging.ImageFormat]::Png)
    Write-Output $destination
} finally {
    $graphics.Dispose(); $sheet.Dispose(); $font.Dispose(); $small.Dispose(); $brush.Dispose()
}
