import React, { useMemo, useState } from "react";
import { Box, Text, useInput, useStdin } from "ink";
import type { AgentDef, ScanResult, ScannedAgent } from "../native.js";

interface AgentsViewProps {
  scan: ScanResult;
  allAgents: AgentDef[];
  isActive: boolean;
}

const WINDOW_SIZE = 14;

export const AgentsView: React.FC<AgentsViewProps> = ({
  scan,
  allAgents,
  isActive,
}) => {
  const { isRawModeSupported } = useStdin();
  const [showAll, setShowAll] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);

  const installedMap = useMemo(() => {
    const map = new Map<string, ScannedAgent>();
    for (const item of scan.installed_agents) {
      map.set(item.key, item);
    }
    return map;
  }, [scan]);

  const rows = useMemo(() => {
    if (showAll) {
      return allAgents.map((def) => ({
        def,
        scanned: installedMap.get(def.key),
        installed: installedMap.has(def.key),
      }));
    }
    return allAgents
      .filter((def) => installedMap.has(def.key))
      .map((def) => ({
        def,
        scanned: installedMap.get(def.key),
        installed: true,
      }));
  }, [showAll, allAgents, installedMap]);

  const safeIndex = rows.length === 0 ? 0 : Math.min(selectedIndex, rows.length - 1);

  useInput(
    (input, key) => {
      if (!isActive) return;
      if (input === "a") {
        setShowAll((prev) => !prev);
        setSelectedIndex(0);
        return;
      }
      if (rows.length === 0) return;
      if (key.upArrow || input === "k") {
        setSelectedIndex((prev) => (prev > 0 ? prev - 1 : rows.length - 1));
      } else if (key.downArrow || input === "j") {
        setSelectedIndex((prev) => (prev + 1) % rows.length);
      }
    },
    { isActive: isActive && Boolean(isRawModeSupported) }
  );

  const start = Math.max(
    0,
    Math.min(safeIndex - Math.floor(WINDOW_SIZE / 2), Math.max(0, rows.length - WINDOW_SIZE))
  );
  const visibleRows = rows.slice(start, start + WINDOW_SIZE);
  const selected = rows[safeIndex];

  return (
    <Box flexDirection="column" gap={1}>
      <Box justifyContent="space-between">
        <Text dimColor>
          显示: {showAll ? `全部已知 (${rows.length})` : `仅已安装 (${rows.length})`} | 按{" "}
          <Text color="cyan">a</Text> 切换全部/已安装 | <Text color="cyan">↑/↓</Text> 选择
        </Text>
      </Box>

      {rows.length === 0 ? (
        <Box borderStyle="round" borderColor="yellow" paddingX={1}>
          <Text color="yellow">
            当前未检测到已安装的 Agent。按 <Text bold>a</Text> 查看全部 {allAgents.length} 个已知 Agent。
          </Text>
        </Box>
      ) : (
        <Box flexDirection="row" gap={2}>
          {/* Left List */}
          <Box
            flexDirection="column"
            width={42}
            borderStyle="round"
            borderColor="cyan"
            paddingX={1}
          >
            <Text bold color="cyan">
              Agents ({safeIndex + 1}/{rows.length})
            </Text>
            {visibleRows.map((row, idx) => {
              const actualIdx = start + idx;
              const isSelected = actualIdx === safeIndex;
              const skillCount = row.scanned
                ? new Set(row.scanned.skills_dirs.flatMap((d) => d.skills)).size
                : 0;
              return (
                <Text
                  key={row.def.key}
                  color={isSelected ? "black" : row.installed ? "white" : "gray"}
                  backgroundColor={isSelected ? "cyan" : undefined}
                >
                  {isSelected ? "› " : "  "}
                  {row.installed ? "● " : "○ "}
                  {row.def.displayName} ({skillCount})
                </Text>
              );
            })}
          </Box>

          {/* Right Details */}
          <Box
            flexDirection="column"
            flexGrow={1}
            borderStyle="round"
            borderColor="gray"
            paddingX={1}
          >
            {selected ? (
              <>
                <Text bold color="cyan">
                  {selected.def.displayName}{" "}
                  <Text color="gray">({selected.def.key})</Text>
                </Text>
                <Text>
                  状态:{" "}
                  {selected.installed ? (
                    <Text color="green">已安装</Text>
                  ) : (
                    <Text color="gray">未检测到</Text>
                  )}{" "}
                  | 分类: {selected.def.category} | 递归扫描:{" "}
                  {selected.def.recursiveScan ? "是" : "否"}
                </Text>

                <Box marginTop={1} flexDirection="column">
                  <Text bold>检测路径 (detect_paths):</Text>
                  {selected.scanned ? (
                    selected.scanned.detect_hits.length === 0 ? (
                      <Text dimColor>  (无固定 detect_paths)</Text>
                    ) : (
                      selected.scanned.detect_hits.map((hit) => (
                        <Text key={hit.configured_path}>
                          {"  "}
                          {hit.exists ? (
                            <Text color="green">✓</Text>
                          ) : (
                            <Text color="gray">✗</Text>
                          )}{" "}
                          {hit.configured_path} [{hit.kind}]
                          {hit.symlink_target ? ` -> ${hit.symlink_target}` : ""}
                        </Text>
                      ))
                    )
                  ) : selected.def.detectPaths.length === 0 ? (
                    <Text dimColor>  (无)</Text>
                  ) : (
                    selected.def.detectPaths.map((p) => (
                      <Text key={p} dimColor>
                        {"  "}○ {p}
                      </Text>
                    ))
                  )}
                </Box>

                <Box marginTop={1} flexDirection="column">
                  <Text bold>Skills 目录:</Text>
                  {selected.scanned ? (
                    selected.scanned.skills_dirs.length === 0 ? (
                      <Text dimColor>  (无全局 Skills 目录)</Text>
                    ) : (
                      selected.scanned.skills_dirs.map((dir) => (
                        <Box key={dir.configured_path} flexDirection="column">
                          <Text>
                            {"  "}
                            {dir.exists ? (
                              <Text color="green">✓</Text>
                            ) : (
                              <Text color="gray">✗</Text>
                            )}{" "}
                            {dir.configured_path} ({dir.skills_count} skills)
                            {dir.symlink_target ? ` -> ${dir.symlink_target}` : ""}
                          </Text>
                          {dir.skills.slice(0, 8).map((s) => (
                            <Text key={s} dimColor>
                              {"      • "}
                              {s}
                            </Text>
                          ))}
                          {dir.skills.length > 8 && (
                            <Text dimColor>
                              {"      ... "}还有 {dir.skills.length - 8} 个
                            </Text>
                          )}
                        </Box>
                      ))
                    )
                  ) : (
                    selected.def.globalSkillsDirs.map((p) => (
                      <Text key={p} dimColor>
                        {"  "}○ {p}
                      </Text>
                    ))
                  )}
                </Box>
              </>
            ) : (
              <Text dimColor>请选择一个 Agent 查看详情</Text>
            )}
          </Box>
        </Box>
      )}
    </Box>
  );
};
