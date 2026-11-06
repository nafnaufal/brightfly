param(
    [string]$Output = (Join-Path $PSScriptRoot "..\assets\brighty.ico")
)

Add-Type -AssemblyName System.Drawing

$sizes = @(16, 20, 24, 32, 40, 48, 64, 256)
$frames = @()

function New-RoundedPath([float]$x, [float]$y, [float]$w, [float]$h, [float]$r) {
    $path = New-Object System.Drawing.Drawing2D.GraphicsPath
    $d = $r * 2
    $path.AddArc($x, $y, $d, $d, 180, 90)
    $path.AddArc($x + $w - $d, $y, $d, $d, 270, 90)
    $path.AddArc($x + $w - $d, $y + $h - $d, $d, $d, 0, 90)
    $path.AddArc($x, $y + $h - $d, $d, $d, 90, 90)
    $path.CloseFigure()
    return $path
}

foreach ($size in $sizes) {
    $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $g.Clear([System.Drawing.Color]::Transparent)

    $margin = [Math]::Max(1.0, $size * 0.045)
    $radius = $size * 0.22
    $background = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 35, 35, 43))
    $path = New-RoundedPath $margin $margin ($size - 2 * $margin) ($size - 2 * $margin) $radius
    $g.FillPath($background, $path)
    $path.Dispose()

    $cx = $size / 2.0
    $cy = $size / 2.0
    $core = $size * 0.155
    $gap = $size * 0.065
    $rayLength = $size * 0.14
    $rayWidth = [Math]::Max(1.4, $size * 0.055)

    $sun = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 216, 212, 245))
    $pen = New-Object System.Drawing.Pen([System.Drawing.Color]::FromArgb(255, 216, 212, 245), $rayWidth)
    $pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
    $pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round

    $inner = $core + $gap
    $outer = $inner + $rayLength
    for ($i = 0; $i -lt 8; $i++) {
        $angle = $i * [Math]::PI / 4.0
        $x1 = $cx + [Math]::Cos($angle) * $inner
        $y1 = $cy + [Math]::Sin($angle) * $inner
        $x2 = $cx + [Math]::Cos($angle) * $outer
        $y2 = $cy + [Math]::Sin($angle) * $outer
        $g.DrawLine($pen, [float]$x1, [float]$y1, [float]$x2, [float]$y2)
    }

    $g.FillEllipse($sun, [float]($cx - $core), [float]($cy - $core), [float]($core * 2), [float]($core * 2))

    $pen.Dispose()
    $sun.Dispose()
    $background.Dispose()
    $g.Dispose()

    if ($size -ge 256) {
        $pngStream = New-Object System.IO.MemoryStream
        $bmp.Save($pngStream, [System.Drawing.Imaging.ImageFormat]::Png)
        $data = $pngStream.ToArray()
        $pngStream.Dispose()
        $bmp.Dispose()
    } else {
        $rect = New-Object System.Drawing.Rectangle(0, 0, $size, $size)
        $bits = $bmp.LockBits($rect, [System.Drawing.Imaging.ImageLockMode]::ReadOnly, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $stride = $bits.Stride
        $pixels = New-Object byte[] ($stride * $size)
        [System.Runtime.InteropServices.Marshal]::Copy($bits.Scan0, $pixels, 0, $pixels.Length)
        $bmp.UnlockBits($bits)
        $bmp.Dispose()

        $xor = New-Object byte[] ($size * $size * 4)
        for ($y = 0; $y -lt $size; $y++) {
            [Array]::Copy($pixels, ($size - 1 - $y) * $stride, $xor, $y * $size * 4, $size * 4)
        }

        $maskStride = [int]([Math]::Ceiling($size / 32.0) * 4)
        $andMask = New-Object byte[] ($maskStride * $size)

        $header = New-Object byte[] 40
        [Array]::Copy([BitConverter]::GetBytes([uint32]40), 0, $header, 0, 4)
        [Array]::Copy([BitConverter]::GetBytes([int32]$size), 0, $header, 4, 4)
        [Array]::Copy([BitConverter]::GetBytes([int32]($size * 2)), 0, $header, 8, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint16]1), 0, $header, 12, 2)
        [Array]::Copy([BitConverter]::GetBytes([uint16]32), 0, $header, 14, 2)
        [Array]::Copy([BitConverter]::GetBytes([uint32]0), 0, $header, 16, 4)
        [Array]::Copy([BitConverter]::GetBytes([uint32]($xor.Length + $andMask.Length)), 0, $header, 20, 4)

        $data = New-Object byte[] ($header.Length + $xor.Length + $andMask.Length)
        [Array]::Copy($header, 0, $data, 0, $header.Length)
        [Array]::Copy($xor, 0, $data, $header.Length, $xor.Length)
        [Array]::Copy($andMask, 0, $data, ($header.Length + $xor.Length), $andMask.Length)
    }

    $frames += [pscustomobject]@{ Size = $size; Data = $data }
}

$dir = Split-Path -Parent $Output
if (-not (Test-Path $dir)) {
    New-Item -ItemType Directory -Path $dir | Out-Null
}

$stream = [System.IO.File]::Create($Output)
$writer = New-Object System.IO.BinaryWriter($stream)
$writer.Write([uint16]0)
$writer.Write([uint16]1)
$writer.Write([uint16]$frames.Count)

$offset = 6 + 16 * $frames.Count
foreach ($frame in $frames) {
    $dim = if ($frame.Size -eq 256) { 0 } else { $frame.Size }
    $writer.Write([byte]$dim)
    $writer.Write([byte]$dim)
    $writer.Write([byte]0)
    $writer.Write([byte]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]32)
    $writer.Write([uint32]$frame.Data.Length)
    $writer.Write([uint32]$offset)
    $offset += $frame.Data.Length
}
foreach ($frame in $frames) {
    $writer.Write($frame.Data)
}
$writer.Dispose()
$stream.Dispose()

Write-Host "Icon written to $Output ($($frames.Count) sizes)"