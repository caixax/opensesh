@echo off
rem Release: release.bat -Patch | -Minor | -Major | -V 1.2.0 [-SkipTests] [-NoPublish] [-SkipLinux] [-InCi]
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0release.ps1" %*
