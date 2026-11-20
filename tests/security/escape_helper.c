/*
 * Adversarial helper for the Marqdo Linux strict security lane.
 *
 * It is deliberately not part of the language runtime: the security test
 * launches this program through the same `marqdo-sandbox` supervisor used by
 * `marqdo run --mode strict`, then checks that direct syscalls cannot escape
 * the filesystem, network, process, privilege or resource boundaries.
 *
 * Modes (argv[1]):
 *   alive   report uid/gid/pid/no-new-privs
 *   read    try host-only files plus the project's allowed.txt
 *   write   try writing outside the project, into the read-only project, /tmp
 *   net     try IPv4/IPv6 connect and a raw socket
 *   proc    try /proc/1/root and inherited-fd probing
 *   priv    try setuid(0), unshare, mount
 *   fds     open /dev/null until EMFILE
 *   fork    fork until the process budget stops us
 *   mem     allocate and touch 768 MiB
 *   cpu     busy loop forever
 *   output  write 4 MiB to stdout
 *
 * The program prints one `key=value` line per observation. A test passes only
 * when sensitive operations report DENIED/limited and the allowed project file
 * is readable.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <arpa/inet.h>
#include <netinet/in.h>

static const char *outside_path;

static void report(const char *key, const char *value) {
    printf("%s=%s\n", key, value);
    fflush(stdout);
}

static void try_read(const char *key, const char *path, int allowed) {
    FILE *file = fopen(path, "r");
    if (file == NULL) {
        report(key, "DENIED");
        return;
    }
    char buffer[64];
    size_t read = fread(buffer, 1, sizeof(buffer) - 1, file);
    buffer[read] = '\0';
    fclose(file);
    /* Only the project's explicitly allowed file is expected to be readable. */
    report(key, allowed ? "OK" : "LEAK");
}

static void try_write(const char *key, const char *path) {
    FILE *file = fopen(path, "w");
    if (file == NULL) {
        report(key, "DENIED");
        return;
    }
    fputs("escaped\n", file);
    fclose(file);
    report(key, "WROTE");
}

static int mode_read(void) {
    try_read("read_etc_shadow", "/etc/shadow", 0);
    try_read("read_root_key", "/root/.ssh/id_ed25519", 0);
    try_read("read_home_key", "/home/dev/.ssh/id_ed25519", 0);
    try_read("read_project_allowed", "/project/allowed.txt", 1);
    return 0;
}

static int mode_write(void) {
    try_write("write_outside", outside_path);
    try_write("write_project", "/project/escape.txt");
    try_write("write_tmp", "/tmp/escape.txt");
    return 0;
}

static int mode_net(void) {
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) {
        report("socket_ipv4", "DENIED");
    } else {
        struct sockaddr_in address;
        memset(&address, 0, sizeof(address));
        address.sin_family = AF_INET;
        address.sin_port = htons(80);
        inet_pton(AF_INET, "1.1.1.1", &address.sin_addr);
        int connected = connect(fd, (struct sockaddr *)&address, sizeof(address));
        report("socket_ipv4", connected == 0 ? "CONNECTED" : "DENIED");
        close(fd);
    }
    int fd6 = socket(AF_INET6, SOCK_STREAM, 0);
    report("socket_ipv6", fd6 < 0 ? "DENIED" : "OPENED");
    if (fd6 >= 0) close(fd6);
    int raw = socket(AF_INET, SOCK_RAW, IPPROTO_ICMP);
    report("socket_raw", raw < 0 ? "DENIED" : "OPENED");
    if (raw >= 0) close(raw);
    return 0;
}

static int mode_proc(void) {
    try_read("proc_pid1_root_etc", "/proc/1/root/etc/shadow", 0);
    try_read("proc_pid1_root_project", "/proc/1/root/project/allowed.txt", 1);
    char path[64];
    int leaks = 0;
    for (int fd = 3; fd < 32; fd++) {
        snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
        struct stat statbuf;
        if (fstat(fd, &statbuf) == 0) {
            leaks++;
        }
    }
    report("inherited_fds_above_stdio", leaks == 0 ? "NONE" : "PRESENT");
    return 0;
}

static int mode_priv(void) {
    report("setuid_root", setuid(0) == 0 ? "OK" : "DENIED");
    report("unshare_userns", unshare(CLONE_NEWUSER) == 0 ? "OK" : "DENIED");
    report("mount_tmpfs", mount("none", "/tmp", "tmpfs", 0, "") == 0 ? "OK" : "DENIED");
    return 0;
}

static int mode_fds(void) {
    int count = 0;
    for (;;) {
        int fd = open("/dev/null", O_RDONLY);
        if (fd < 0) break;
        count++;
        if (count > 4096) break;
    }
    printf("open_fds=%d\n", count);
    report("fd_limit", count <= 64 ? "BOUNDED" : "UNBOUNDED");
    return 0;
}

static int mode_fork(void) {
    int children = 0;
    for (int i = 0; i < 200; i++) {
        pid_t pid = fork();
        if (pid < 0) break;
        if (pid == 0) {
            _exit(0);
        }
        children++;
    }
    for (int i = 0; i < children; i++) {
        wait(NULL);
    }
    printf("forked=%d\n", children);
    report("process_limit", children < 200 ? "BOUNDED" : "UNBOUNDED");
    return 0;
}

static int mode_mem(void) {
    size_t size = 768UL * 1024 * 1024;
    char *memory = malloc(size);
    if (memory == NULL) {
        report("malloc_768m", "DENIED");
        return 0;
    }
    for (size_t offset = 0; offset < size; offset += 4096) {
        memory[offset] = 1;
    }
    report("malloc_768m", "OK");
    free(memory);
    return 0;
}

static int mode_cpu(void) {
    volatile unsigned long counter = 0;
    for (;;) {
        counter++;
    }
    return (int)counter;
}

static int mode_output(void) {
    char block[65536];
    memset(block, 'x', sizeof(block));
    for (int i = 0; i < 64; i++) {
        if (fwrite(block, 1, sizeof(block), stdout) != sizeof(block)) {
            report("output_4m", "DENIED");
            return 3;
        }
    }
    fflush(stdout);
    report("output_4m", "WRITTEN");
    return 0;
}

static int mode_alive(void) {
    printf("uid=%d gid=%d pid=%d\n", (int)getuid(), (int)getgid(), (int)getpid());
    FILE *status = fopen("/proc/self/status", "r");
    char line[256];
    while (status != NULL && fgets(line, sizeof(line), status) != NULL) {
        if (strncmp(line, "NoNewPrivs:", 11) == 0 ||
            strncmp(line, "CapEff:", 7) == 0) {
            printf("%s", line);
        }
    }
    if (status != NULL) fclose(status);
    return 0;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "usage: %s MODE [OUTSIDE_PATH]\n", argv[0]);
        return 2;
    }
    outside_path = argc > 2 ? argv[2] : "/home/dev/escape-outside";
    const char *mode = argv[1];
    if (strcmp(mode, "alive") == 0) return mode_alive();
    if (strcmp(mode, "read") == 0) return mode_read();
    if (strcmp(mode, "write") == 0) return mode_write();
    if (strcmp(mode, "net") == 0) return mode_net();
    if (strcmp(mode, "proc") == 0) return mode_proc();
    if (strcmp(mode, "priv") == 0) return mode_priv();
    if (strcmp(mode, "fds") == 0) return mode_fds();
    if (strcmp(mode, "fork") == 0) return mode_fork();
    if (strcmp(mode, "mem") == 0) return mode_mem();
    if (strcmp(mode, "cpu") == 0) return mode_cpu();
    if (strcmp(mode, "output") == 0) return mode_output();
    fprintf(stderr, "unknown mode: %s\n", mode);
    return 2;
}
