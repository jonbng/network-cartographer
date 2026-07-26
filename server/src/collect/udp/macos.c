#include <arpa/inet.h>
#include <CoreFoundation/CoreFoundation.h>
#include <errno.h>
#include <libproc.h>
#include <netinet/in.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/proc_info.h>

size_t nc_bundle_display_name(const char *path, uint8_t *output, size_t capacity) {
    if (path == NULL || output == NULL || capacity == 0) return 0;
    output[0] = 0;
    CFURLRef url = CFURLCreateFromFileSystemRepresentation(
        kCFAllocatorDefault, (const UInt8 *)path, (CFIndex)strlen(path), true);
    if (url == NULL) return 0;
    CFBundleRef bundle = CFBundleCreate(kCFAllocatorDefault, url);
    CFRelease(url);
    if (bundle == NULL) return 0;

    CFStringRef value = NULL;
    CFDictionaryRef localized = CFBundleGetLocalInfoDictionary(bundle);
    if (localized != NULL) {
        value = (CFStringRef)CFDictionaryGetValue(localized, CFSTR("CFBundleDisplayName"));
        if (value == NULL) {
            value = (CFStringRef)CFDictionaryGetValue(localized, CFSTR("CFBundleName"));
        }
    }
    if (value == NULL) {
        CFDictionaryRef info = CFBundleGetInfoDictionary(bundle);
        if (info != NULL) {
            value = (CFStringRef)CFDictionaryGetValue(info, CFSTR("CFBundleDisplayName"));
            if (value == NULL) {
                value = (CFStringRef)CFDictionaryGetValue(info, CFSTR("CFBundleName"));
            }
        }
    }

    size_t written = 0;
    if (value != NULL && CFGetTypeID(value) == CFStringGetTypeID() &&
        CFStringGetCString(value, (char *)output, (CFIndex)capacity, kCFStringEncodingUTF8)) {
        written = strlen((const char *)output);
    }
    CFRelease(bundle);
    return written;
}

struct nc_socket_peer {
    uint32_t pid;
    uint8_t protocol;
    uint8_t family;
    uint8_t state;
    uint8_t reserved;
    uint16_t local_port;
    uint16_t remote_port;
    uint8_t local_addr[16];
    uint8_t remote_addr[16];
};

struct nc_scan_stats {
    size_t matched;
    size_t written;
    uint32_t inaccessible_processes;
    uint32_t transient_processes;
};

static int address_is_zero(const uint8_t *address, size_t length) {
    for (size_t index = 0; index < length; index++) {
        if (address[index] != 0) return 0;
    }
    return 1;
}

static void classify_process_error(int error, int *inaccessible, int *transient) {
    if (error == EACCES || error == EPERM) {
        *inaccessible = 1;
    } else if (error == ESRCH || error == EBADF) {
        *transient = 1;
    }
}

int nc_collect_sockets(int include_udp, struct nc_socket_peer *output,
                       size_t capacity, struct nc_scan_stats *stats) {
    if (output == NULL || stats == NULL) return EINVAL;
    memset(stats, 0, sizeof(*stats));
    int pid_bytes = proc_listpids(PROC_ALL_PIDS, 0, NULL, 0);
    if (pid_bytes <= 0) return errno ? errno : EIO;
    size_t pid_capacity = (size_t)pid_bytes / sizeof(pid_t);
    pid_t *pids = calloc(pid_capacity, sizeof(pid_t));
    if (pids == NULL) return ENOMEM;
    int pid_result = proc_listpids(PROC_ALL_PIDS, 0, pids,
                                  pid_bytes);
    if (pid_result <= 0) {
        int error = errno ? errno : EIO;
        free(pids);
        return error;
    }

    size_t pid_count = (size_t)pid_result / sizeof(pid_t);
    for (size_t pid_index = 0; pid_index < pid_count; pid_index++) {
        pid_t pid = pids[pid_index];
        if (pid <= 0) continue;
        int inaccessible = 0;
        int transient = 0;
        errno = 0;
        int fd_bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, NULL, 0);
        if (fd_bytes <= 0) {
            classify_process_error(errno, &inaccessible, &transient);
            stats->inaccessible_processes += (uint32_t)inaccessible;
            stats->transient_processes += (uint32_t)transient;
            continue;
        }
        struct proc_fdinfo *fds = malloc((size_t)fd_bytes);
        if (fds == NULL) {
            free(pids);
            return ENOMEM;
        }
        errno = 0;
        fd_bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, fds, fd_bytes);
        if (fd_bytes <= 0) {
            classify_process_error(errno, &inaccessible, &transient);
            free(fds);
            stats->inaccessible_processes += (uint32_t)inaccessible;
            stats->transient_processes += (uint32_t)transient;
            continue;
        }

        size_t fd_count = (size_t)fd_bytes / sizeof(struct proc_fdinfo);
        for (size_t fd_index = 0; fd_index < fd_count; fd_index++) {
            if (fds[fd_index].proc_fdtype != PROX_FDTYPE_SOCKET) continue;
            struct socket_fdinfo info;
            errno = 0;
            int size = proc_pidfdinfo(pid, fds[fd_index].proc_fd,
                                      PROC_PIDFDSOCKETINFO, &info, sizeof(info));
            if (size != sizeof(info)) {
                classify_process_error(errno, &inaccessible, &transient);
                continue;
            }
            if (info.psi.soi_protocol != IPPROTO_TCP &&
                !(include_udp && info.psi.soi_protocol == IPPROTO_UDP)) continue;

            struct in_sockinfo inet;
            uint8_t state = 0;
            if (info.psi.soi_protocol == IPPROTO_TCP && info.psi.soi_kind == SOCKINFO_TCP) {
                struct tcp_sockinfo tcp = info.psi.soi_proto.pri_tcp;
                inet = tcp.tcpsi_ini;
                state = (uint8_t)tcp.tcpsi_state;
            } else if (info.psi.soi_protocol == IPPROTO_UDP && info.psi.soi_kind == SOCKINFO_IN) {
                inet = info.psi.soi_proto.pri_in;
            } else {
                continue;
            }

            struct nc_socket_peer peer;
            memset(&peer, 0, sizeof(peer));
            peer.pid = (uint32_t)pid;
            peer.protocol = (uint8_t)info.psi.soi_protocol;
            peer.family = (uint8_t)info.psi.soi_family;
            peer.state = state;
            peer.local_port = ntohs((uint16_t)inet.insi_lport);
            peer.remote_port = ntohs((uint16_t)inet.insi_fport);
            if (peer.remote_port == 0) continue;

            size_t address_length;
            if (info.psi.soi_family == AF_INET) {
                address_length = 4;
                memcpy(peer.local_addr, &inet.insi_laddr.ina_46.i46a_addr4, 4);
                memcpy(peer.remote_addr, &inet.insi_faddr.ina_46.i46a_addr4, 4);
            } else if (info.psi.soi_family == AF_INET6) {
                address_length = 16;
                memcpy(peer.local_addr, &inet.insi_laddr.ina_6, 16);
                memcpy(peer.remote_addr, &inet.insi_faddr.ina_6, 16);
            } else {
                continue;
            }
            if (address_is_zero(peer.remote_addr, address_length)) continue;
            if (stats->written < capacity) {
                output[stats->written++] = peer;
            }
            stats->matched++;
        }
        free(fds);
        stats->inaccessible_processes += (uint32_t)inaccessible;
        stats->transient_processes += (uint32_t)transient;
    }
    free(pids);
    return 0;
}
