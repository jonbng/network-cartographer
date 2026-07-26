#import <AppKit/AppKit.h>
#include <errno.h>
#include <libproc.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/proc_info.h>
#include <unistd.h>

struct nc_process_identity {
    uint32_t parent_pid;
    uint64_t start_time;
    uint32_t user_id;
    int32_t session_id;
    char name[256];
    char path[PROC_PIDPATHINFO_MAXSIZE];
};

int nc_read_process_identity(int pid, struct nc_process_identity *output) {
    if (pid <= 0 || output == NULL) return EINVAL;
    memset(output, 0, sizeof(*output));
    output->session_id = -1;

    struct proc_bsdinfo info;
    memset(&info, 0, sizeof(info));
    int read = proc_pidinfo(pid, PROC_PIDTBSDINFO, 0, &info, sizeof(info));
    if (read != sizeof(info)) return errno ? errno : ESRCH;

    output->parent_pid = info.pbi_ppid;
    output->start_time = info.pbi_start_tvsec;
    output->user_id = info.pbi_uid;
    pid_t session = getsid((pid_t)pid);
    if (session >= 0) output->session_id = (int32_t)session;

    if (info.pbi_name[0] != '\0') {
        strlcpy(output->name, info.pbi_name, sizeof(output->name));
    } else {
        proc_name(pid, output->name, (uint32_t)sizeof(output->name));
    }
    proc_pidpath(pid, output->path, (uint32_t)sizeof(output->path));
    return 0;
}

int nc_copy_app_icon_png(const char *path, uint8_t **output, size_t *length) {
    if (path == NULL || output == NULL || length == NULL) return EINVAL;
    *output = NULL;
    *length = 0;

    @autoreleasepool {
        NSString *file = [NSString stringWithUTF8String:path];
        if (file == nil) return EINVAL;
        NSImage *icon = [[NSWorkspace sharedWorkspace] iconForFile:file];
        if (icon == nil) return ENOENT;

        const NSInteger side = 64;
        NSBitmapImageRep *bitmap = [[[NSBitmapImageRep alloc]
            initWithBitmapDataPlanes:NULL
                          pixelsWide:side
                          pixelsHigh:side
                       bitsPerSample:8
                     samplesPerPixel:4
                            hasAlpha:YES
                            isPlanar:NO
                      colorSpaceName:NSCalibratedRGBColorSpace
                         bytesPerRow:0
                        bitsPerPixel:0] autorelease];
        if (bitmap == nil) return ENOMEM;

        NSGraphicsContext *context = [NSGraphicsContext graphicsContextWithBitmapImageRep:bitmap];
        if (context == nil) return EIO;
        [NSGraphicsContext saveGraphicsState];
        [NSGraphicsContext setCurrentContext:context];
        [icon drawInRect:NSMakeRect(0, 0, side, side)
                fromRect:NSZeroRect
               operation:NSCompositingOperationSourceOver
                fraction:1.0
          respectFlipped:YES
                   hints:nil];
        [context flushGraphics];
        [NSGraphicsContext restoreGraphicsState];

        NSData *png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
        if (png == nil || png.length == 0) return EIO;
        uint8_t *copy = malloc(png.length);
        if (copy == NULL) return ENOMEM;
        memcpy(copy, png.bytes, png.length);
        *output = copy;
        *length = png.length;
        return 0;
    }
}

void nc_free_buffer(void *buffer) {
    free(buffer);
}
