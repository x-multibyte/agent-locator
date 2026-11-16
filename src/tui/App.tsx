import React, { useCallback, useEffect, useState } from "react";
import { Box, Text, useApp, useInput, useStdin } from "ink";
import {
  getSkillsMatrix,
  runDoctor,
  scanAgents,
  type DoctorReport,
  type ScanResult,
  type SkillsMatrixReport,
} from "../native.js";
import { AgentsView } from "./AgentsView.js";
import { SkillsMatrixView } from "./SkillsMatrixView.js";
import { DoctorView } from "./DoctorView.js";

export interface AppData {
  scan: ScanResult;
  matrix: SkillsMatrixReport;
  doctor: DoctorReport;
}

interface AppProps {
  registryPath?: string;
  initialData?: AppData;
  disableExitOnQ?: boolean;
}

type TabKey = "agents" | "skills" | "doctor";

export const App: React.FC<AppProps> = ({
  registryPath,
  initialData,
  disableExitOnQ = false,
}) => {
  const { exit } = useApp();
  const { isRawModeSupported } = useStdin();
  const [activeTab, setActiveTab] = useState<TabKey>("agents");
  const [data, setData] = useState<AppData | null>(initialData ?? null);
  const [loading, setLoading] = useState(!initialData);
  const [error, setError] = useState<string | null>(null);
  const [statusMessage, setStatusMessage] = useState<string>(
    "就绪 — 使用 1/2/3 或 Tab 切换视图，r 刷新，q 退出"
  );

  const loadAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [scan, matrix, doctor] = await Promise.all([
        scanAgents(registryPath),
        getSkillsMatrix(registryPath),
        runDoctor({ registryPath, fix: false }),
      ]);
      setData({ scan, matrix, doctor });
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setLoading(false);
    }
  }, [registryPath]);

  useEffect(() => {
    if (!initialData) {
      void loadAll();
    }
  }, [initialData, loadAll]);

  useInput(
    (input, key) => {
      if (input === "q" && !disableExitOnQ) {
        exit();
        return;
      }
      if (input === "1") {
        setActiveTab("agents");
      } else if (input === "2") {
        setActiveTab("skills");
      } else if (input === "3") {
        setActiveTab("doctor");
      } else if (key.tab) {
        setActiveTab((prev) =>
          prev === "agents" ? "skills" : prev === "skills" ? "doctor" : "agents"
        );
      } else if (input === "r") {
        void loadAll().then(() => setStatusMessage("✓ 已异步刷新扫描数据"));
      }
    },
    { isActive: Boolean(isRawModeSupported) }
  );

  if (loading && !data) {
    return (
      <Box padding={1} flexDirection="column">
        <Text color="cyan" bold>
          Agent Locator (agloc)
        </Text>
        <Text>正在通过 Rust 原生核心异步扫描本机 Agent 与 Skills...</Text>
      </Box>
    );
  }

  if (error && !data) {
    return (
      <Box padding={1} flexDirection="column">
        <Text color="red" bold>
          扫描失败: {error}
        </Text>
        <Text dimColor>按 r 重试，按 q 退出</Text>
      </Box>
    );
  }

  if (!data) {
    return null;
  }

  const tabs: Array<{ key: TabKey; num: string; label: string }> = [
    {
      key: "agents",
      num: "1",
      label: `Agents 概览 (${data.scan.summary.installed_count}/${data.scan.summary.total_checked})`,
    },
    {
      key: "skills",
      num: "2",
      label: `Skills 矩阵 (${data.matrix.skills.length})`,
    },
    {
      key: "doctor",
      num: "3",
      label: `Doctor 巡检 (${data.doctor.totalIssues})`,
    },
  ];

  return (
    <Box flexDirection="column" padding={1} gap={1}>
      {/* Header */}
      <Box justifyContent="space-between" borderStyle="single" borderColor="cyan" paddingX={1}>
        <Text bold color="cyan">
          Agent Locator (agloc) — Rust + Ink
        </Text>
        <Text dimColor>中央仓库: {data.matrix.centralHubDir}</Text>
      </Box>

      {/* Tab Bar */}
      <Box flexDirection="row" gap={2}>
        {tabs.map((tab) => {
          const active = tab.key === activeTab;
          return (
            <Text
              key={tab.key}
              bold={active}
              color={active ? "black" : "white"}
              backgroundColor={active ? "cyan" : undefined}
            >
              {" "}
              [{tab.num}] {tab.label}{" "}
            </Text>
          );
        })}
      </Box>

      {/* Active View */}
      {activeTab === "agents" && (
        <AgentsView
          scan={data.scan}
          allAgents={data.matrix.allAgents}
          isActive={activeTab === "agents"}
        />
      )}
      {activeTab === "skills" && (
        <SkillsMatrixView
          matrix={data.matrix}
          isActive={activeTab === "skills"}
          onRefresh={loadAll}
          onStatus={setStatusMessage}
        />
      )}
      {activeTab === "doctor" && (
        <DoctorView
          doctor={data.doctor}
          isActive={activeTab === "doctor"}
          onRefresh={loadAll}
          onStatus={setStatusMessage}
        />
      )}

      {/* Footer Status */}
      <Box borderStyle="single" borderColor="gray" paddingX={1} justifyContent="space-between">
        <Text>{statusMessage}</Text>
        <Text dimColor>[Tab/1-3] 切页 | [r] 刷新 | [q] 退出</Text>
      </Box>
    </Box>
  );
};
