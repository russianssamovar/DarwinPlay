/* DarwinPlay steamwebhelper shim.
 *
 * Wine has no cross-process rendering support outside winex11.drv: when a
 * window's top-level owner lives in another process, update_visible_region()
 * in win32u/dce.c leaves the surface NULL, and winemac.drv registers no pGetDC
 * to compensate the way X11DRV_GetDC does. Chromium/CEF presents into HWNDs
 * owned by a remote process, so every Steam CEF window paints into nothing and
 * renders black while still handling input.
 *
 * Running the GPU/viz thread inside the browser process removes that process
 * boundary. Steam does not forward --in-process-gpu from its own command line,
 * and Steam re-runs steamwebhelper.exe for every CEF child process, so the flag
 * has to be injected here.
 *
 * Installed as steamwebhelper.exe with the original renamed to
 * steamwebhelper_real.exe. Steam restores the original on client update and on
 * file verification, so DarwinPlay reinstalls this before every launch and
 * passes -noverifyfiles.
 *
 * Build:
 *   x86_64-w64-mingw32-gcc -O1 -municode -mwindows \
 *       -o steamwebhelper-shim.exe steamwebhelper-shim.c
 */
#include <windows.h>
#include <stdio.h>
#include <wchar.h>

/* Lets DarwinPlay recognise its own shim without depending on file size. */
__attribute__((used))
static const char darwinplay_shim_marker[] = "DARWINPLAY_SWH_SHIM_V1";

#define REAL_BINARY   L"steamwebhelper_real.exe"
#define DEFAULT_FLAGS L"--in-process-gpu"

/* Skip argv[0] in a raw command line, honouring quoting. */
static const WCHAR *skip_argv0(const WCHAR *cmd)
{
    BOOL quoted = FALSE;
    while (*cmd == L' ' || *cmd == L'\t') cmd++;
    if (*cmd == L'"') { quoted = TRUE; cmd++; }
    while (*cmd)
    {
        if (quoted) { if (*cmd == L'"') { cmd++; break; } }
        else if (*cmd == L' ' || *cmd == L'\t') break;
        cmd++;
    }
    while (*cmd == L' ' || *cmd == L'\t') cmd++;
    return cmd;
}

int WINAPI wWinMain(HINSTANCE inst, HINSTANCE prev, LPWSTR argline, int show)
{
    WCHAR dir[MAX_PATH], real[MAX_PATH * 2], flags[4096];
    static WCHAR cmdline[32768];
    WCHAR *slash;
    STARTUPINFOW si;
    PROCESS_INFORMATION pi;
    DWORD code = 1;

    (void)darwinplay_shim_marker;
    (void)inst; (void)prev; (void)argline; (void)show;

    if (!GetModuleFileNameW(NULL, dir, MAX_PATH)) return 1;
    if ((slash = wcsrchr(dir, L'\\'))) *(slash + 1) = 0;

    _snwprintf(real, MAX_PATH * 2, L"%s%s", dir, REAL_BINARY);

    if (!GetEnvironmentVariableW(L"DARWINPLAY_SWH_FLAGS", flags, 4096))
        wcscpy(flags, DEFAULT_FLAGS);

    _snwprintf(cmdline, 32768, L"\"%s\" %s %s",
               real, skip_argv0(GetCommandLineW()), flags);

    ZeroMemory(&si, sizeof(si));
    si.cb = sizeof(si);
    ZeroMemory(&pi, sizeof(pi));

    /* Chromium passes inherited handle values on the command line, so the
     * grandchild must inherit them too. */
    if (!CreateProcessW(real, cmdline, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi))
        return 1;

    WaitForSingleObject(pi.hProcess, INFINITE);
    GetExitCodeProcess(pi.hProcess, &code);
    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return (int)code;
}
