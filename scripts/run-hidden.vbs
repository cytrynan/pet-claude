' run-hidden.vbs -- run a console program with NO visible window (for Task Scheduler / Startup).
'
' Usage (Task Scheduler action):
'   Program : C:\Windows\System32\wscript.exe
'   Args    : //B //Nologo "%USERPROFILE%\.claude\scripts\run-hidden.vbs" <logname> <exe> [args...]
'
' - stdout/stderr of the program are APPENDED to %USERPROFILE%\.claude\logs\hidden\<logname>.log
'   (each run is prefixed with a timestamp line, so a silent run is still visible as a line).
' - This script WAITS for the program and returns its exit code, so Task Scheduler's
'   "Last Run Result" and MultipleInstances=IgnoreNew keep working.
' - Working directory is inherited from the task (set "Start in" on the task if the program needs it).
' - Why VBS and not "powershell -WindowStyle Hidden": powershell still flashes a console for a moment,
'   and node/python/cmd started directly by Task Scheduler (Interactive logon) always open a console.
'   WScript.Shell.Run(cmd, 0, True) creates the console hidden from the start.
' - NOTE: this file must stay ASCII (wscript reads .vbs as ANSI/CP932; UTF-8 Japanese becomes mojibake).

Option Explicit

Dim sh, fso, args, i, cmd, logDir, logFile, rc, ts, stamp

Set sh  = CreateObject("WScript.Shell")
Set fso = CreateObject("Scripting.FileSystemObject")
Set args = WScript.Arguments

If args.Count < 2 Then
  WScript.Echo "usage: run-hidden.vbs <logname> <exe> [args...]"
  WScript.Quit 2
End If

logDir = sh.ExpandEnvironmentStrings("%USERPROFILE%") & "\.claude\logs"
If Not fso.FolderExists(logDir) Then fso.CreateFolder logDir
logDir = logDir & "\hidden"
If Not fso.FolderExists(logDir) Then fso.CreateFolder logDir
logFile = logDir & "\" & args(0) & ".log"

cmd = ""
For i = 1 To args.Count - 1
  cmd = cmd & " """ & args(i) & """"
Next
cmd = Trim(cmd)

stamp = Year(Now) & "-" & Right("0" & Month(Now), 2) & "-" & Right("0" & Day(Now), 2) & "T" & _
        Right("0" & Hour(Now), 2) & ":" & Right("0" & Minute(Now), 2) & ":" & Right("0" & Second(Now), 2)
On Error Resume Next
Set ts = fso.OpenTextFile(logFile, 8, True)
ts.WriteLine "[" & stamp & "] run-hidden: " & cmd
ts.Close
On Error GoTo 0

' cmd.exe strips the outermost pair of quotes, leaving:  "exe" "arg" ... >> "log" 2>&1
rc = sh.Run("cmd.exe /d /c """ & cmd & " >> """ & logFile & """ 2>&1""", 0, True)
WScript.Quit rc
