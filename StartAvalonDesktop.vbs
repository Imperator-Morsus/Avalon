Set WshShell = CreateObject("WScript.Shell")
Set FSO = CreateObject("Scripting.FileSystemObject")

scriptDir = FSO.GetParentFolderName(WScript.ScriptFullName)
clientDir = FSO.BuildPath(scriptDir, "client")

' Check for release build first, fallback to debug
releaseExe = FSO.BuildPath(scriptDir, "target\release\avalon_backend.exe")
debugExe = FSO.BuildPath(scriptDir, "target\debug\avalon_backend.exe")

avalonExe = ""
If FSO.FileExists(releaseExe) Then
    avalonExe = releaseExe
ElseIf FSO.FileExists(debugExe) Then
    avalonExe = debugExe
End If

If avalonExe = "" Then
    MsgBox "Avalon backend not found. Please run 'cargo build --release' first.", vbCritical, "Avalon Desktop"
    WScript.Quit 1
End If

' Check if node_modules exists
If Not FSO.FolderExists(FSO.BuildPath(clientDir, "node_modules")) Then
    result = MsgBox("Electron dependencies not found. Install now?" & vbCrLf & vbCrLf & "This may take a few minutes.", vbYesNo + vbQuestion, "Avalon Desktop")
    If result = vbYes Then
        WshShell.CurrentDirectory = clientDir
        Dim installCmd
        installCmd = "cmd.exe /c npm install"
        WshShell.Run installCmd, 1, True
    Else
        WScript.Quit 1
    End If
End If

' Start Electron app (backend will be started by Electron's main.js)
WshShell.CurrentDirectory = clientDir
Dim startCmd
startCmd = "cmd.exe /c npm start"
WshShell.Run startCmd, 0, False

' Non-blocking notification that auto-closes after 3 seconds
WshShell.Popup "Avalon Desktop is starting..." & vbCrLf & vbCrLf & "This window will close automatically.", 3, "Avalon Desktop", 0

' Do NOT kill processes on exit — Electron handles its own lifecycle
