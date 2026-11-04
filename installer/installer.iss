; Inno Setup: Ezpectral Comp by NoTempo (Windows). Compile from the repo root:
;   ISCC.exe installer\installer.iss
[Setup]
AppName=Ezpectral Comp
AppVersion=1.0.0
AppPublisher=NoTempo
DefaultDirName={commoncf64}\VST3\NoTempo
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
OutputDir=..\target
OutputBaseFilename=Ezpectral-Comp-Installer
Compression=lzma2
SolidCompression=yes

[Components]
Name: "vst3"; Description: "VST3"; Types: full custom
Name: "clap"; Description: "CLAP"; Types: full custom

[Files]
Source: "..\target\bundled\Ezpectral Comp.vst3\*"; DestDir: "{commoncf64}\VST3\NoTempo\Ezpectral Comp.vst3"; Flags: recursesubdirs ignoreversion; Components: vst3
Source: "..\target\bundled\Ezpectral Comp.clap"; DestDir: "{commoncf64}\CLAP\NoTempo"; Flags: ignoreversion; Components: clap
