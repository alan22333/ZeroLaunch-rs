#!/usr/bin/env bash
# ============================================================================
# summarize-changes commit message 行长度校验脚本
# 从 stdin 读取 commit message，逐行检测长度：
#   - 首行（header）≤ HEADER_MAX（默认 72，conventional commit 标准）
#   - 其余行（body）≤ BODY_MAX（默认 100，commitlint body-max-line-length）
# 全部通过 → 退出码 0；存在超限行 → 打印违规明细与行号，退出码 1。
#
# 不使用 wc（跨平台性）：wc -l 在无尾换行输出时会少计一行。
# 长度统计用 perl 的 length()（配合 -Mutf8 按 Unicode 字符计数），
# 而非 bash 的 ${#}——bash 内建多字节支持依赖编译期 MB_CUR_MAX，
# Git Bash / MSYS2 下按字节计数，中文会误报超长。perl 是
# Git for Windows、macOS、Linux 的标准组件，行为一致。
#
# 用法：lint-commit.sh [HEADER_MAX] [BODY_MAX]
# ============================================================================
set -euo pipefail

HEADER_MAX="${1:-72}"
BODY_MAX="${2:-100}"

# perl 逐行读取 stdin，输出超限行的行号与字符数；无超限则无输出。
# 退出码：0 = 合规，1 = 存在超限行。
violations=$(perl -Mutf8 -CSD -e '
    my ($header_max, $body_max) = @ARGV;
    my $line_no = 0;
    my $bad = 0;
    while (my $line = <STDIN>) {
        $line_no++;
        $line =~ s/\r$//;   # 去除 Windows 行尾 CR
        chomp $line;
        my $max = $line_no == 1 ? $header_max : $body_max;
        my $len = length($line);
        if ($len > $max) {
            print "✖ 第 ${line_no} 行超长：${len} 字符（上限 ${max}）\n";
            print "  ${line}\n";
            $bad = 1;
        }
    }
    if ($line_no == 0) {
        print "✖ 输入为空，未检测到 commit message。\n";
        exit 1;
    }
    exit $bad ? 1 : 0;
' "$HEADER_MAX" "$BODY_MAX") || true

if [ -n "$violations" ]; then
    echo "$violations"
    echo ""
    echo "存在超限行，请压缩措辞后重新生成 commit message。"
    exit 1
fi

echo "✅ commit message 行长度合规（header ≤${HEADER_MAX}，body ≤${BODY_MAX}）。"
