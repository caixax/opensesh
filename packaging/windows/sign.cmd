@echo off
rem Code-signs the files it is given with the certificate the release workflow imported into the
rem user's store (SIGN_THUMBPRINT), using signtool (SIGNTOOL) and an RFC 3161 timestamp server
rem (SIGN_TIMESTAMP_URL, DigiCert's by default). `cargo xtask dist windows` runs it when
rem OPENSESH_SIGN points here (ADR 0039).
if not defined SIGNTOOL set SIGNTOOL=signtool
if not defined SIGN_THUMBPRINT (
    echo sign.cmd: SIGN_THUMBPRINT is not set 1>&2
    exit /b 1
)
if not defined SIGN_TIMESTAMP_URL set SIGN_TIMESTAMP_URL=http://timestamp.digicert.com
"%SIGNTOOL%" sign /sha1 %SIGN_THUMBPRINT% /fd sha256 /tr %SIGN_TIMESTAMP_URL% /td sha256 %*
