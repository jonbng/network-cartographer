#include <arpa/inet.h>
#include <errno.h>
#include <libproc.h>
#include <netinet/in.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include <sys/proc_info.h>

struct nc_udp_peer {
    uint32_t pid;
    uint8_t family;
    uint8_t reserved[3];
    uint16_t local_port;
    uint16_t remote_port;
    uint8_t local_addr[16];
    uint8_t remote_addr[16];
};

struct nc_tcp_peer {
    uint32_t pid;
    uint8_t family;
    uint8_t state;
    uint8_t reserved[2];
    uint16_t local_port;
    uint16_t remote_port;
    uint8_t local_addr[16];
    uint8_t remote_addr[16];
};

static int address_is_zero(const uint8_t *address, size_t length) {
    for (size_t index = 0; index < length; index++) {
        if (address[index] != 0) return 0;
    }
    return 1;
}

int nc_collect_udp(struct nc_udp_peer *output, size_t capacity, size_t *written) {
    if (output == NULL || written == NULL) return EINVAL;
    *written = 0;

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
    for (size_t pid_index = 0; pid_index < pid_count && *written < capacity; pid_index++) {
        pid_t pid = pids[pid_index];
        if (pid <= 0) continue;
        int fd_bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, NULL, 0);
        if (fd_bytes <= 0) continue;
        struct proc_fdinfo *fds = malloc((size_t)fd_bytes);
        if (fds == NULL) continue;
        fd_bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, fds, fd_bytes);
        if (fd_bytes <= 0) {
            free(fds);
            continue;
        }

        size_t fd_count = (size_t)fd_bytes / sizeof(struct proc_fdinfo);
        for (size_t fd_index = 0; fd_index < fd_count && *written < capacity; fd_index++) {
            if (fds[fd_index].proc_fdtype != PROX_FDTYPE_SOCKET) continue;
            struct socket_fdinfo info;
            int size = proc_pidfdinfo(pid, fds[fd_index].proc_fd,
                                      PROC_PIDFDSOCKETINFO, &info, sizeof(info));
            if (size != sizeof(info) || info.psi.soi_protocol != IPPROTO_UDP ||
                info.psi.soi_kind != SOCKINFO_IN) continue;

            struct in_sockinfo inet = info.psi.soi_proto.pri_in;
            struct nc_udp_peer peer;
            memset(&peer, 0, sizeof(peer));
            peer.pid = (uint32_t)pid;
            peer.family = (uint8_t)info.psi.soi_family;
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
            output[(*written)++] = peer;
        }
        free(fds);
    }
    free(pids);
    return 0;
}

int nc_collect_tcp(struct nc_tcp_peer *output, size_t capacity, size_t *written) {
    if (output == NULL || written == NULL) return EINVAL;
    *written = 0;
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
    for (size_t pid_index = 0; pid_index < pid_count && *written < capacity; pid_index++) {
        pid_t pid = pids[pid_index];
        if (pid <= 0) continue;
        int fd_bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, NULL, 0);
        if (fd_bytes <= 0) continue;
        struct proc_fdinfo *fds = malloc((size_t)fd_bytes);
        if (fds == NULL) continue;
        fd_bytes = proc_pidinfo(pid, PROC_PIDLISTFDS, 0, fds, fd_bytes);
        if (fd_bytes <= 0) { free(fds); continue; }
        size_t fd_count = (size_t)fd_bytes / sizeof(struct proc_fdinfo);
        for (size_t fd_index = 0; fd_index < fd_count && *written < capacity; fd_index++) {
            if (fds[fd_index].proc_fdtype != PROX_FDTYPE_SOCKET) continue;
            struct socket_fdinfo info;
            int size = proc_pidfdinfo(pid, fds[fd_index].proc_fd,
                                      PROC_PIDFDSOCKETINFO, &info, sizeof(info));
            if (size != sizeof(info) || info.psi.soi_protocol != IPPROTO_TCP ||
                info.psi.soi_kind != SOCKINFO_TCP) continue;
            struct tcp_sockinfo tcp = info.psi.soi_proto.pri_tcp;
            struct in_sockinfo inet = tcp.tcpsi_ini;
            struct nc_tcp_peer peer;
            memset(&peer, 0, sizeof(peer));
            peer.pid = (uint32_t)pid;
            peer.family = (uint8_t)info.psi.soi_family;
            peer.state = (uint8_t)tcp.tcpsi_state;
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
            } else continue;
            if (address_is_zero(peer.remote_addr, address_length)) continue;
            output[(*written)++] = peer;
        }
        free(fds);
    }
    free(pids);
    return 0;
}
