import React, { useState } from "react";
import { Box, Text, useInput, useStdin, useStdout } from "ink";
import {
  distributeSkill,
  importSkill,
  undistributeSkill,
  type SkillMetadata,
  type SkillsMatrixReport,
} from "../native.js";

interface SkillsMatrixViewProps {
  matrix: SkillsMatrixReport;
  isActive: boolean;
  onRefresh: () => Promise<void>;
  onStatus: (msg: string) => void;
}

export const SkillsMatrixView: React.FC<SkillsMatrixViewProps> = ({
  matrix,
  isActive,
  onRefresh,
  onStatus,
}) => {
  const { isRawModeSupported } = useStdin();
  const { stdout } = useStdout();
  const [focusPane, setFocusPane] = useState<"skills" | "agents">("skills");
  const [skillIndex, setSkillIndex] = useState(0);
  const [agentIndex, setAgentIndex] = useState(0);
  const [defaultMode, setDefaultMode] = useState<"symlink" | "copy">("symlink");
  const [busy, setBusy] = useState(false);

  const termRows = stdout?.rows ?? 28;
  const paneHeight = Math.max(14, Math.min(22, termRows - 14));
  const skillPageSize = Math.max(6, paneHeight - 3);
  const agentPageSize = Math.max(5, paneHeight - 7);

  const skills = matrix.skills;
  const agents = matrix.installedAgents;

  const safeSkillIdx = skills.length === 0 ? 0 : Math.min(skillIndex, skills.length - 1);
  const safeAgentIdx = agents.length === 0 ? 0 : Math.min(agentIndex, agents.length - 1);
  const selectedSkill: SkillMetadata | undefined = skills[safeSkillIdx];
  const selectedAgent = agents[safeAgentIdx];

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

      if (key.leftArrow || input === "h") {
        setFocusPane("skills");
        return;
      }
      if (key.rightArrow || input === "l") {
        if (agents.length > 0) setFocusPane("agents");
        return;
      }
      if (key.return) {
        setFocusPane((prev) =>
          prev === "skills" && agents.length > 0 ? "agents" : "skills"
        );
        return;
      }

      if (focusPane === "skills") {
        if (skills.length === 0) return;
        if (key.upArrow || input === "k") {
          setSkillIndex((prev) => (prev > 0 ? prev - 1 : skills.length - 1));
        } else if (key.downArrow || input === "j") {
          setSkillIndex((prev) => (prev + 1) % skills.length);
        } else if (key.pageUp || input === "[") {
          setSkillIndex((prev) => Math.max(0, prev - skillPageSize));
        } else if (key.pageDown || input === "]") {
          setSkillIndex((prev) => Math.min(skills.length - 1, prev + skillPageSize));
        } else if (input === "i" && selectedSkill && !selectedSkill.inCentralHub) {
          const sourcePlacement = selectedSkill.placements.find(
            (p) => p.status === "unmanaged"
          );
          if (sourcePlacement) {
            void runAction(
              `收纳 '${selectedSkill.id}' 至中央仓库 (${defaultMode})`,
              async () => {
                await importSkill(sourcePlacement.agentKey, selectedSkill.id, defaultMode);
              }
            );
          }
        } else if (input === "m") {
          const next = defaultMode === "symlink" ? "copy" : "symlink";
          setDefaultMode(next);
          onStatus(`默认分发模式已切换为: ${next}`);
        }
        return;
      }

      // focusPane === "agents"
      if (agents.length === 0 || !selectedSkill || !selectedAgent) return;
      if (key.upArrow || input === "k") {
        setAgentIndex((prev) => (prev > 0 ? prev - 1 : agents.length - 1));
      } else if (key.downArrow || input === "j") {
        setAgentIndex((prev) => (prev + 1) % agents.length);
      } else if (key.pageUp || input === "[") {
        setAgentIndex((prev) => Math.max(0, prev - agentPageSize));
      } else if (key.pageDown || input === "]") {
        setAgentIndex((prev) => Math.min(agents.length - 1, prev + agentPageSize));
      } else if (input === " ") {
        const placement = selectedSkill.placements.find(
          (p) => p.agentKey === selectedAgent.key
        );
        if (!selectedSkill.inCentralHub) {
          if (placement && placement.status === "unmanaged") {
            void runAction(
              `收纳 '${selectedSkill.id}' (${defaultMode})`,
              async () => {
                await importSkill(selectedAgent.key, selectedSkill.id, defaultMode);
              }
            );
          } else {
            onStatus("请先按 i 将该 Skill 收纳至中央仓库后再分发到其他 Agent");
          }
          return;
        }
        if (placement) {
          void runAction(
            `从 ${selectedAgent.displayName} 移除 '${selectedSkill.id}'`,
            async () => {
              await undistributeSkill(selectedSkill.id, [selectedAgent.key]);
            }
          );
        } else {
          void runAction(
            `分发 '${selectedSkill.id}' 到 ${selectedAgent.displayName} (${defaultMode})`,
            async () => {
              await distributeSkill(selectedSkill.id, [selectedAgent.key], defaultMode);
            }
          );
        }
      } else if (input === "m") {
        const placement = selectedSkill.placements.find(
          (p) => p.agentKey === selectedAgent.key
        );
        if (placement && selectedSkill.inCentralHub && placement.mode !== "unmanaged") {
          const targetMode = placement.mode === "symlink" ? "copy" : "symlink";
          void runAction(
            `切换 '${selectedSkill.id}' 在 ${selectedAgent.displayName} 的模式为 ${targetMode}`,
            async () => {
              await distributeSkill(selectedSkill.id, [selectedAgent.key], targetMode);
            }
          );
        } else {
          const next = defaultMode === "symlink" ? "copy" : "symlink";
          setDefaultMode(next);
          onStatus(`默认分发模式已切换为: ${next}`);
        }
      } else if (input === "i" && !selectedSkill.inCentralHub) {
        const placement = selectedSkill.placements.find(
          (p) => p.agentKey === selectedAgent.key && p.status === "unmanaged"
        );
        if (placement) {
          void runAction(
            `收纳 '${selectedSkill.id}' (${defaultMode})`,
            async () => {
              await importSkill(selectedAgent.key, selectedSkill.id, defaultMode);
            }
          );
        }
      }
    },
    { isActive: isActive && Boolean(isRawModeSupported) }
  );

  const skillPage = Math.floor(safeSkillIdx / skillPageSize);
  const skillTotalPages = Math.max(1, Math.ceil(skills.length / skillPageSize));
  const skillStart = skillPage * skillPageSize;
  const visibleSkills = skills.slice(skillStart, skillStart + skillPageSize);

  const agentPage = Math.floor(safeAgentIdx / agentPageSize);
  const agentTotalPages = Math.max(1, Math.ceil(agents.length / agentPageSize));
  const agentStart = agentPage * agentPageSize;
  const visibleAgents = agents.slice(agentStart, agentStart + agentPageSize);

  const normalizedDesc = selectedSkill?.description
    ? selectedSkill.description.replace(/\s+/g, " ").trim()
    : "";

  return (
    <Box flexDirection="column" gap={1}>
      <Text dimColor wrap="truncate-end">
        焦点: <Text color="cyan">{focusPane === "skills" ? "左侧 Skills" : "右侧 Agents"}</Text>{" "}
        | 默认模式: <Text color="magenta">{defaultMode}</Text> |{" "}
        <Text color="cyan">←/→/Enter</Text> 切栏 | <Text color="cyan">[/]/PgUp/PgDn</Text> 翻页 |{" "}
        <Text color="cyan">Space</Text> 挂载/移除 | <Text color="cyan">m</Text> 切换模式 |{" "}
        <Text color="cyan">i</Text> 收纳入库
      </Text>

      {skills.length === 0 ? (
        <Box borderStyle="round" borderColor="yellow" paddingX={1} flexDirection="column">
          <Text color="yellow">
            中央仓库 ({matrix.centralHubDir}) 与已安装 Agent 中暂未发现 Skill。
          </Text>
          <Text dimColor>
            可使用 CLI 命令 `agloc skills add &lt;本地路径&gt;` 添加 Skill。
          </Text>
        </Box>
      ) : (
        <Box flexDirection="row" gap={2}>
          {/* Left Skills List */}
          <Box
            flexDirection="column"
            width={44}
            flexShrink={0}
            height={paneHeight}
            overflow="hidden"
            borderStyle="round"
            borderColor={focusPane === "skills" ? "cyan" : "gray"}
            paddingX={1}
          >
            <Text bold color={focusPane === "skills" ? "cyan" : "white"} wrap="truncate-end">
              Skills ({safeSkillIdx + 1}/{skills.length}) · 第 {skillPage + 1}/{skillTotalPages} 页
            </Text>
            {visibleSkills.map((s, idx) => {
              const actualIdx = skillStart + idx;
              const isSelected = actualIdx === safeSkillIdx;
              const tag = s.inCentralHub ? "●" : "○";
              return (
                <Text
                  key={s.id}
                  wrap="truncate-end"
                  color={isSelected ? "black" : s.inCentralHub ? "white" : "yellow"}
                  backgroundColor={isSelected ? "cyan" : undefined}
                >
                  {isSelected ? "› " : "  "}
                  {tag} {s.id} ({s.placements.length})
                </Text>
              );
            })}
          </Box>

          {/* Right Skill Details & Target Agents Matrix */}
          <Box
            flexDirection="column"
            flexGrow={1}
            flexBasis={0}
            height={paneHeight}
            overflow="hidden"
            borderStyle="round"
            borderColor={focusPane === "agents" ? "cyan" : "gray"}
            paddingX={1}
          >
            {selectedSkill ? (
              <>
                <Text bold color="cyan" wrap="truncate-end">
                  {selectedSkill.name || selectedSkill.id}{" "}
                  <Text color="gray">[{selectedSkill.id}]</Text>
                </Text>
                <Text wrap="truncate-end">
                  仓库状态:{" "}
                  {selectedSkill.inCentralHub ? (
                    <Text color="green">已入库 (~/.agent-locator/skills)</Text>
                  ) : (
                    <Text color="yellow">未纳管 (按 i 一键收纳)</Text>
                  )}
                  {" | "}SKILL.md:{" "}
                  {selectedSkill.hasSkillMd ? (
                    <Text color="green">有效</Text>
                  ) : (
                    <Text color="red">缺失</Text>
                  )}
                </Text>
                {normalizedDesc && (
                  <Text dimColor wrap="truncate-end">
                    简介: {normalizedDesc}
                  </Text>
                )}

                <Box marginTop={1} flexDirection="column" overflow="hidden">
                  <Text bold color={focusPane === "agents" ? "cyan" : "white"} wrap="truncate-end">
                    目标 Agents 分发状态 ({agents.length === 0 ? 0 : safeAgentIdx + 1}/{agents.length} · 第 {agentPage + 1}/{agentTotalPages} 页):
                  </Text>
                  {agents.length === 0 ? (
                    <Text dimColor>  未检测到已安装的 Agent</Text>
                  ) : (
                    visibleAgents.map((agent, visibleIdx) => {
                      const idx = agentStart + visibleIdx;
                      const placement = selectedSkill.placements.find(
                        (p) => p.agentKey === agent.key
                      );
                      const isSelected = focusPane === "agents" && idx === safeAgentIdx;
                      const checkbox = placement ? "[✓]" : "[ ]";
                      const modeText = placement
                        ? `${placement.mode} (${placement.status})`
                        : "未分发";
                      const modeColor = !placement
                        ? "gray"
                        : placement.status === "synced"
                        ? "green"
                        : placement.status === "drifted" || placement.status === "broken"
                        ? "red"
                        : "yellow";
                      return (
                        <Text
                          key={agent.key}
                          wrap="truncate-end"
                          color={isSelected ? "black" : undefined}
                          backgroundColor={isSelected ? "cyan" : undefined}
                        >
                          {isSelected ? "› " : "  "}
                          {checkbox} {agent.displayName}{" "}
                          <Text color={isSelected ? "black" : modeColor}>
                            — {modeText}
                          </Text>
                        </Text>
                      );
                    })
                  )}
                </Box>
              </>
            ) : (
              <Text dimColor>请选择一个 Skill</Text>
            )}
          </Box>
        </Box>
      )}
    </Box>
  );
};
