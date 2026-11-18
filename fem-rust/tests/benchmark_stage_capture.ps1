# Shared benchmark-only capture of flushed stdout stage markers. Timestamps are
# observed by the parent process, not internal solver instrumentation.
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Text;

public sealed class FemBenchmarkStageLine {
    public double elapsedMs { get; set; }
    public string line { get; set; }
}

public sealed class FemBenchmarkOutputCapture {
    private readonly object gate = new object();
    private readonly Stopwatch clock;
    private readonly StringBuilder output = new StringBuilder();
    private readonly List<FemBenchmarkStageLine> stages = new List<FemBenchmarkStageLine>();
    public FemBenchmarkOutputCapture(Stopwatch clock) { this.clock = clock; }
    public void Attach(Process process) {
        process.OutputDataReceived += (sender, e) => {
            if (e.Data == null) return;
            double elapsed = clock.Elapsed.TotalMilliseconds;
            lock (gate) {
                output.AppendLine(e.Data);
                if (e.Data.StartsWith("FULLMETA_STAGE ", StringComparison.Ordinal))
                    stages.Add(new FemBenchmarkStageLine { elapsedMs = elapsed, line = e.Data.Substring(15) });
            }
        };
    }
    public string Output { get { lock (gate) return output.ToString(); } }
    public FemBenchmarkStageLine[] Stages { get { lock (gate) return stages.ToArray(); } }
}
'@

function Get-StageIntervals($Capture, [double]$ExitMs) {
    $events = @($Capture.Stages | ForEach-Object {
        $event = $_.line | ConvertFrom-Json
        if ($event.event -eq 'stage-start') {
            [pscustomobject]@{ stage = [string]$event.stage; startMs = $_.elapsedMs }
        }
    })
    $intervals = for ($i = 0; $i -lt $events.Count; $i++) {
        $end = if ($i + 1 -lt $events.Count) { $events[$i+1].startMs } else { $ExitMs }
        [pscustomobject]@{
            stage = $events[$i].stage; startMs = $events[$i].startMs
            elapsedMs = [Math]::Max([double]0, [double]($end - $events[$i].startMs))
            endsAt = if ($i + 1 -lt $events.Count) { 'next-stage' } else { 'process-exit' }
        }
    }
    return ,@($intervals)
}

function Get-StructuredTriangleGraphEntries([int]$CellsPerSide) {
    return 7*$CellsPerSide*$CellsPerSide + 6*$CellsPerSide + 1
}

# Independently enumerate local triangle pairs on small grids. This catches
# mistakes in the closed-form count used for larger benchmark meshes.
foreach ($n in @(2, 3, 4)) {
    $pairs = [System.Collections.Generic.HashSet[string]]::new()
    for ($y = 0; $y -lt $n; $y++) {
        for ($x = 0; $x -lt $n; $x++) {
            $a = $y*($n+1)+$x; $b = $a+1; $c = $a+$n+1; $d = $c+1
            foreach ($triangle in @(@($a,$b,$c), @($b,$d,$c))) {
                foreach ($i in $triangle) { foreach ($j in $triangle) { [void]$pairs.Add("$i,$j") } }
            }
        }
    }
    if ($pairs.Count -ne (Get-StructuredTriangleGraphEntries $n)) {
        throw "Triangle graph count disagrees with independent enumeration for n=$n."
    }
}
