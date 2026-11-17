import React, { useState } from "react";
import { Box, Text, useInput, useStdin, useStdout } from "ink";
import {
  importSkill,
  runDoctor,
  syncAllSkills,
  type DoctorIssue,
  type DoctorReport,
} from "../native.js";

interface DoctorViewProps {
  doctor: DoctorReport;
  isActive: boolean;
  onRefresh: () => Promise<void>;
  onStatus: (msg: string) => void;
}

export const DoctorView: React.FC<DoctorViewProps> = ({
  doctor,
  isActive,
  onRefresh,
  onStatus,
}) => {
  const { isRawModeSupported } = useStdin();
  const { stdout } = useStdout();
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [busy, setBusy] = useState(false);

  const termRows = stdout?.rows ?? 28;
  const paneHeight = Math.max(14, Math.min(22, termRows - 14));
  const windowSize = Math.max(6, paneHeight - 3);

  const issues = doctor.issues;
  const safeIdx = issues.length === 0 ? 0 : Math.min(selectedIndex, issues.length - 1);
  const selectedIssue: DoctorIssue | undefined = issues[safeIdx];

  const runAction = async (label: string, fn: () => Promise<void>) => {
    setBusy(true);
    onStatus(`正在执行: ${label}...`);
    try {
      await fn();
      await onRefresh();
      onStatus(`✓ ${label}`);
    } catch (err) {
      onStatus(`✗ 失败: ${(err as Error).message}`);
    } finally {
      setBusy(false);
    }
  };

  useInput(
    (input, key) => {
      if (!isActive || busy) return;

      if (issues.length > 0) {
        if (key.upArrow || input === "k") {
          setSelectedIndex((prev) => (prev > 0 ? prev - 1 : issues.length - 1));
          return;
        }
        if (key.downArrow || input === "j") {
          setSelectedIndex((prev) => (prev + 1) % issues.length);
          return;
        }
        if (key.pageUp || input === "[") {
          setSelectedIndex((prev) => Math.max(0, prev - windowSize));
          return;
        }
        if (key.pageDown || input === "]") {
          setSelectedIndex((prev) => Math.min(issues.length - 1, prev + windowSize));
          return;
        }
      }

      if (input === "i" && selectedIssue?.issueType === "unmanaged_skill" && selectedIssue.agentKey) {
        const agentKey = selectedIssue.agentKey;
        const skillId = selectedIssue.skillId;
        void runAction(`收纳 '${skillId}' (${agentKey})`, async () => {
          await importSkill(agentKey, skillId, "symlink");
        });
      } else if (input === "s") {
        void runAction("同步所有受管 Skills", async () => {
          await syncAllSkills();
        });
      } else if (input === "f") {
        void runAction("自动修复所有可修复问题", async () => {
          await runDoctor({ fix: true });
        });
      }
    },
    { isActive: isActive && Boolean(isRawModeSupported) }
  );

  const currentPage = Math.floor(safeIdx / windowSize);
  const totalPages = Math.max(1, Math.ceil(issues.length / windowSize));
  const start = currentPage * windowSize;
  const visibleIssues = issues.slice(start, start + windowSize);

  return (
    <Box flexDirection="column" gap={1}>
      <Text dimColor wrap="truncate-end">
        <Text color="cyan">↑/↓/[/]/PgUp/PgDn</Text> 翻页 | <Text color="cyan">i</Text> 收纳选中的野生 Skill |{" "}
        <Text color="cyan">s</Text> 同步偏移副本 | <Text color="cyan">f</Text> 一键全部修复
      </Text>

      {issues.length === 0 ? (
        <Box borderStyle="round" borderColor="green" paddingX={1}>
          <Text color="green">
            ✓ 所有检查均通过，未发现失效软链接、副本偏移、野生 Skill 或缺失 SKILL.md 的情况。
          </Text>
        </Box>
      ) : (
        <Box
          flexDirection="column"
          height={paneHeight}
          overflow="hidden"
          borderStyle="round"
          borderColor="yellow"
          paddingX={1}
        >
          <Text bold color="yellow" wrap="truncate-end">
            发现 {issues.length} 个待处理项 ({safeIdx + 1}/{issues.length} · 第 {currentPage + 1}/{totalPages} 页):
          </Text>
          {visibleIssues.map((issue, idx) => {
            const actualIdx = start + idx;
            const isSelected = actualIdx === safeIdx;
            const badgeColor =
              issue.severity === "error"
                ? "red"
                : issue.severity === "warning"
                ? "yellow"
                : "cyan";
            return (
              <Text
                key={`${issue.issueType}-${issue.path}-${actualIdx}`}
                wrap="truncate-end"
                color={isSelected ? "black" : undefined}
                backgroundColor={isSelected ? "cyan" : undefined}
              >
                {isSelected ? "› " : "  "}
                <Text color={isSelected ? "black" : badgeColor}>
                  [{issue.issueType}]
                </Text>{" "}
                {issue.message}
              </Text>
            );
          })}
        </Box>
      )}
    </Box>
  );
};
