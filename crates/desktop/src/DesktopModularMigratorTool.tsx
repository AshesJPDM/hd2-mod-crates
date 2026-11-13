import { useState, useEffect, useMemo, useCallback } from "react";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import ArchiveIcon from "@mui/icons-material/Archive";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import WarningAmberIcon from "@mui/icons-material/WarningAmber";
import Checkbox from "@mui/material/Checkbox";
import FormControlLabel from "@mui/material/FormControlLabel";
import Radio from "@mui/material/Radio";
import RadioGroup from "@mui/material/RadioGroup";
import Switch from "@mui/material/Switch";
import TextField from "@mui/material/TextField";
import Autocomplete from "@mui/material/Autocomplete";
import Button from "@mui/material/Button";
import Chip from "@mui/material/Chip";
import LinearProgress from "@mui/material/LinearProgress";
import CircularProgress from "@mui/material/CircularProgress";
import { toast } from "sonner";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import {
  chooseModularFolder,
  chooseModularZip,
  chooseOutputZip,
  inspectModularMod,
  loadEquipmentOptions,
  previewModularMappings,
  startModularMigration,
} from "./desktopClient";
import { waitForDesktopTask } from "./desktopTask";
import type {
  EquipmentOption,
  ModularModInspection,
  ModularSlotMapping,
  ModularMigrationSummary,
  MigrationProgressEvent,
} from "./types";

export function DesktopModularMigratorTool() {
  // Source Mod State
  const [modPath, setModPath] = useState<string | null>(null);
  const [materialPath, setMaterialPath] = useState<string | null>(null);
  const [inspection, setInspection] = useState<ModularModInspection | null>(null);
  const [inspecting, setInspecting] = useState(false);

  // Target Equipment Options
  const [equipmentList, setEquipmentList] = useState<EquipmentOption[]>([]);
  const [targetArmor, setTargetArmor] = useState<EquipmentOption | null>(null);
  const [targetHelmet, setTargetHelmet] = useState<EquipmentOption | null>(null);

  // Strategy & Configuration
  const [strategy, setStrategy] = useState<"full_topology" | "smart_merge">("full_topology");
  const [antiPollution, setAntiPollution] = useState(false);
  const [selectedComponents, setSelectedComponents] = useState<string[]>([]);
  const [customModName, setCustomModName] = useState("");

  // Preview Mappings
  const [mappings, setMappings] = useState<ModularSlotMapping[]>([]);
  const [loadingMappings, setLoadingMappings] = useState(false);

  // Task / Migration Execution State
  const [busy, setBusy] = useState(false);
  const [progressStage, setProgressStage] = useState("");
  const [progressPercent, setProgressPercent] = useState<number | null>(null);
  const [summary, setSummary] = useState<ModularMigrationSummary | null>(null);

  // Load equipment options on mount
  useEffect(() => {
    loadEquipmentOptions()
      .then((opts) => {
        setEquipmentList(opts);
        // Default target armor: CW-9 White Wolf if available
        const defaultTarget = opts.find(
          (o) => o.name.includes("CW-9") || o.name.includes("White Wolf"),
        );
        if (defaultTarget) {
          setTargetArmor(defaultTarget);
        }
      })
      .catch((err) => toast.error(`加载护甲列表失败: ${err}`));
  }, []);

  // Filter armors and helmets
  const armorOptions = useMemo(
    () => equipmentList.filter((e) => e.category === "Armor"),
    [equipmentList],
  );
  const helmetOptions = useMemo(
    () => equipmentList.filter((e) => e.category === "Helmet"),
    [equipmentList],
  );

  // Inspect mod when modPath or materialPath changes
  useEffect(() => {
    if (!modPath) {
      setInspection(null);
      setSelectedComponents([]);
      return;
    }
    setInspecting(true);
    inspectModularMod({ modPath, materialPackPath: materialPath })
      .then((res) => {
        setInspection(res);
        setSelectedComponents(res.components.map((c) => c.name));
        setCustomModName(
          targetArmor ? `${res.modName} 替换 ${targetArmor.name}` : res.modName,
        );
      })
      .catch((err) => {
        toast.error(`解析Mod文件失败: ${err}`);
        setInspection(null);
      })
      .finally(() => setInspecting(false));
  }, [modPath, materialPath]);

  // Update custom mod name when target armor changes
  useEffect(() => {
    if (inspection && targetArmor) {
      setCustomModName(`${inspection.modName} 替换 ${targetArmor.name}`);
    }
  }, [inspection, targetArmor]);

  // Preview slot mappings
  useEffect(() => {
    if (!targetArmor) {
      setMappings([]);
      return;
    }
    const srcArmor = inspection?.detectedSourceArmor ?? "TG-8 Sharpshooter";
    setLoadingMappings(true);
    previewModularMappings({
      sourceArmor: srcArmor,
      targetArmor: targetArmor.name,
      sourceHelmet: inspection?.detectedSourceHelmet ?? null,
      targetHelmet: targetHelmet?.name ?? null,
      customOverrides: {},
    })
      .then((res) => setMappings(res))
      .catch((err) => console.error("Preview mappings error:", err))
      .finally(() => setLoadingMappings(false));
  }, [inspection?.detectedSourceArmor, inspection?.detectedSourceHelmet, targetArmor, targetHelmet]);

  // Handle Pick Mod Folder
  const handlePickModFolder = useCallback(async () => {
    const path = await chooseModularFolder();
    if (path) setModPath(path);
  }, []);

  // Handle Pick Mod ZIP
  const handlePickModZip = useCallback(async () => {
    const path = await chooseModularZip();
    if (path) setModPath(path);
  }, []);

  // Handle Pick Material Pack
  const handlePickMaterialPack = useCallback(async () => {
    const path = await chooseModularFolder();
    if (path) {
      setMaterialPath(path);
    } else {
      const zipPath = await chooseModularZip();
      if (zipPath) setMaterialPath(zipPath);
    }
  }, []);

  // Component toggle
  const toggleComponent = useCallback((name: string) => {
    setSelectedComponents((prev) =>
      prev.includes(name) ? prev.filter((c) => c !== name) : [...prev, name],
    );
  }, []);

  // Run Migration
  const handleRunMigration = useCallback(async () => {
    if (!modPath || !targetArmor) {
      toast.error("请先选择Mod源文件和目标护甲");
      return;
    }

    const defaultZipName = `${customModName || "HD2_Modular_Migrated_Mod"}.zip`;
    const outputPath = await chooseOutputZip(defaultZipName);
    if (!outputPath) return;

    setBusy(true);
    setProgressStage("正在准备迁移...");
    setProgressPercent(null);
    setSummary(null);

    try {
      const task = startModularMigration(
        {
          modPath,
          materialPackPath: materialPath,
          outputPath,
          options: {
            targetArmorName: targetArmor.name,
            targetHelmetName: targetHelmet?.name ?? null,
            selectedComponents,
            strategy,
            customUnitMappings: {},
            antiPollution,
            outputModName: customModName,
          },
        },
        (event: MigrationProgressEvent) => {
          if (event.kind === "stage") {
            setProgressStage(`处理阶段: ${event.stage}`);
          } else if (event.kind === "outputProgress" && event.totalBytes > 0) {
            setProgressPercent(Math.round((event.completedBytes / event.totalBytes) * 100));
          }
        },
      );

      const abortController = new AbortController();
      const result = await waitForDesktopTask(task, abortController.signal);
      setSummary(result);
      toast.success("多部件护甲迁移成功！已生成Arsenal专用Mod包。");
    } catch (err) {
      toast.error(`迁移失败: ${err}`);
    } finally {
      setBusy(false);
    }
  }, [modPath, targetArmor, targetHelmet, materialPath, selectedComponents, strategy, antiPollution, customModName]);

  return (
    <div className="flex flex-col gap-6 p-6">
      {/* 1. Source Mod Selection Card */}
      <div className="rounded-lg border border-hd2-border bg-hd2-surface/50 p-5 shadow-lg">
        <div className="mb-4 flex items-center justify-between border-b border-hd2-border pb-3">
          <div className="flex items-center gap-2">
            <ArchiveIcon className="text-hd2-yellow" />
            <h2 className="m-0 text-base font-bold text-hd2-text">1. 选择多部件Mod源文件与材质包</h2>
          </div>
          {inspecting && (
            <div className="flex items-center gap-2 text-xs text-hd2-yellow">
              <CircularProgress size="1rem" color="inherit" />
              <span>正在解析Mod部件结构...</span>
            </div>
          )}
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {/* Mod Source */}
          <div className="rounded border border-hd2-border/60 bg-black/40 p-4">
            <div className="text-sm font-semibold text-hd2-yellow">护甲Mod文件 (包含各部件开关)</div>
            <p className="my-1.5 text-xs text-hd2-muted">
              支持直接选择解压后的Mod文件夹或未解压的 .zip 压缩包。
            </p>
            <div className="mt-3 flex flex-wrap gap-2">
              <Button
                variant="contained"
                startIcon={<FolderOpenIcon />}
                onClick={handlePickModFolder}
                size="small"
              >
                选择Mod文件夹
              </Button>
              <Button
                variant="outlined"
                startIcon={<ArchiveIcon />}
                onClick={handlePickModZip}
                size="small"
              >
                选择Mod压缩包 (.zip)
              </Button>
            </div>
            {modPath && (
              <p className="mt-3 break-all font-mono text-xs text-emerald-400">
                已载入: {modPath}
              </p>
            )}
          </div>

          {/* Material Pack Source */}
          <div className="rounded border border-hd2-border/60 bg-black/40 p-4">
            <div className="text-sm font-semibold text-hd2-yellow">独立材质包 (可选)</div>
            <p className="my-1.5 text-xs text-hd2-muted">
              若该Mod的贴图或材质包为独立存放（如单独的材质包文件夹），可在此一并选取整合。
            </p>
            <div className="mt-3 flex items-center gap-2">
              <Button
                variant="outlined"
                startIcon={<FolderOpenIcon />}
                onClick={handlePickMaterialPack}
                size="small"
              >
                选择材质包
              </Button>
              {materialPath && (
                <Button
                  color="warning"
                  onClick={() => setMaterialPath(null)}
                  size="small"
                >
                  清除
                </Button>
              )}
            </div>
            {materialPath ? (
              <p className="mt-3 break-all font-mono text-xs text-emerald-400">
                已载入材质包: {materialPath}
              </p>
            ) : (
              <p className="mt-3 text-xs text-hd2-faint">未选择单独材质包（Mod内置贴图则无需选择）</p>
            )}
          </div>
        </div>

        {/* Inspection Summary */}
        {inspection && (
          <div className="mt-4 rounded border border-hd2-yellow/30 bg-hd2-yellow/5 p-4">
            <div className="grid grid-cols-1 gap-2 text-xs sm:grid-cols-2 md:grid-cols-4">
              <div>
                <span className="text-hd2-muted">原Mod名称: </span>
                <span className="font-semibold text-hd2-text">{inspection.modName}</span>
              </div>
              <div>
                <span className="text-hd2-muted">识别原目标护甲: </span>
                <span className="font-semibold text-hd2-yellow">
                  {inspection.detectedSourceArmor ?? "未知护甲 (通用匹配)"}
                </span>
              </div>
              <div>
                <span className="text-hd2-muted">识别独立头盔/面部: </span>
                <span className="font-semibold text-hd2-yellow">
                  {inspection.detectedSourceHelmet ?? "无独立头部"}
                </span>
              </div>
              <div>
                <span className="text-hd2-muted">Arsenal清单: </span>
                <span className="font-semibold text-emerald-400">
                  {inspection.hasManifest ? "标准 manifest.json" : "已自动生成"}
                </span>
              </div>
            </div>

            {/* Components Selection Checklist */}
            <div className="mt-4 border-t border-hd2-border/60 pt-3">
              <div className="mb-2 text-xs font-bold text-hd2-text">
                检测到的服饰/部件开关 ({inspection.components.length} 个部件，勾选需要保留的部件):
              </div>
              <div className="flex flex-wrap gap-2">
                {inspection.components.map((comp) => {
                  const checked = selectedComponents.includes(comp.name);
                  return (
                    <div
                      key={comp.name}
                      onClick={() => toggleComponent(comp.name)}
                      className={`flex cursor-pointer items-center gap-1.5 rounded border px-2.5 py-1 text-xs transition-colors ${
                        checked
                          ? "border-hd2-yellow bg-hd2-yellow/20 text-hd2-yellow"
                          : "border-hd2-line bg-black/40 text-hd2-muted opacity-60"
                      }`}
                    >
                      <Checkbox
                        size="small"
                        checked={checked}
                        sx={{ p: 0, color: "inherit" }}
                      />
                      <span className="font-medium">{comp.name}</span>
                      <span className="text-[0.625rem] text-hd2-faint">
                        ({comp.patchFiles.length} files)
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          </div>
        )}
      </div>

      {/* 2. Target Equipment Selection */}
      <div className="rounded-lg border border-hd2-border bg-hd2-surface/50 p-5 shadow-lg">
        <div className="mb-4 flex items-center justify-between border-b border-hd2-border pb-3">
          <div className="flex items-center gap-2">
            <CheckCircleIcon className="text-hd2-yellow" />
            <h2 className="m-0 text-base font-bold text-hd2-text">2. 选择目标护甲与头部</h2>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
          {/* Target Armor Autocomplete */}
          <div>
            <label className="mb-1 block text-xs font-semibold text-hd2-yellow">
              目标移植护甲 (例如 CW-9 白狼)
            </label>
            <Autocomplete
              options={armorOptions}
              getOptionLabel={(opt) => opt.name}
              value={targetArmor}
              onChange={(_, val) => setTargetArmor(val)}
              renderInput={(params) => (
                <TextField
                  {...params}
                  placeholder="搜索护甲名称..."
                  size="small"
                  sx={{
                    "& .MuiOutlinedInput-root": {
                      color: "#fff",
                      backgroundColor: "rgba(0,0,0,0.4)",
                    },
                  }}
                />
              )}
            />
          </div>

          {/* Target Helmet Autocomplete */}
          <div>
            <label className="mb-1 block text-xs font-semibold text-hd2-yellow">
              目标头盔 (若Mod包含面部发型则一并替换为该头盔)
            </label>
            <Autocomplete
              options={helmetOptions}
              getOptionLabel={(opt) => opt.name}
              value={targetHelmet}
              onChange={(_, val) => setTargetHelmet(val)}
              renderInput={(params) => (
                <TextField
                  {...params}
                  placeholder="默认使用目标护甲对应头盔..."
                  size="small"
                  sx={{
                    "& .MuiOutlinedInput-root": {
                      color: "#fff",
                      backgroundColor: "rgba(0,0,0,0.4)",
                    },
                  }}
                />
              )}
            />
          </div>
        </div>

        {/* Custom Mod Name Field */}
        <div className="mt-4">
          <label className="mb-1 block text-xs font-semibold text-hd2-muted">
            迁移后的Mod名称 (用于Arsenal管理器显示与文件名)
          </label>
          <TextField
            fullWidth
            size="small"
            value={customModName}
            onChange={(e) => setCustomModName(e.target.value)}
            sx={{
              "& .MuiOutlinedInput-root": {
                color: "#fff",
                backgroundColor: "rgba(0,0,0,0.4)",
              },
            }}
          />
        </div>
      </div>

      {/* 3. Strategy & Anti-Pollution Settings */}
      <div className="rounded-lg border border-hd2-border bg-hd2-surface/50 p-5 shadow-lg">
        <div className="mb-4 flex items-center justify-between border-b border-hd2-border pb-3">
          <div className="flex items-center gap-2">
            <WarningAmberIcon className="text-hd2-yellow" />
            <h2 className="m-0 text-base font-bold text-hd2-text">3. 迁移策略与防污染设置</h2>
          </div>
        </div>

        <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
          {/* Strategy Selection */}
          <div>
            <div className="text-xs font-bold text-hd2-yellow">移植策略选择:</div>
            <RadioGroup
              value={strategy}
              onChange={(e) => setStrategy(e.target.value as "full_topology" | "smart_merge")}
              className="mt-2"
            >
              <FormControlLabel
                value="full_topology"
                control={<Radio size="small" color="warning" />}
                label={
                  <div className="text-xs">
                    <span className="font-semibold text-hd2-text">完整拓扑映射 (推荐)</span>
                    <p className="m-0 text-hd2-muted">
                      映射目标护甲的全部23个插槽（10个身体骨骼部件 + 3个衣物独立开关 + 1个头盔面部 + 9个饰品挂载点），保持原Mod全部离线服饰开关。
                    </p>
                  </div>
                }
              />
              <FormControlLabel
                value="smart_merge"
                control={<Radio size="small" color="warning" />}
                label={
                  <div className="text-xs">
                    <span className="font-semibold text-hd2-text">智能合并挂载 (自定义模式)</span>
                    <p className="m-0 text-hd2-muted">
                      根据目标护甲可用插槽数量自动分配或自由指定挂载点，适合挂载部件数量受限的护甲。
                    </p>
                  </div>
                }
              />
            </RadioGroup>
          </div>

          {/* Anti-Pollution Guard */}
          <div className="rounded border border-hd2-border/60 bg-black/40 p-4">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-hd2-yellow">防护甲污染模式 (Anti-Pollution Guard)</span>
              <Switch
                checked={antiPollution}
                onChange={(e) => setAntiPollution(e.target.checked)}
                color="warning"
                size="small"
              />
            </div>
            <p className="my-2 text-xs leading-5 text-hd2-muted">
              部分护甲与其他护甲共用手臂或鞋袜等底层模型资产（资产复用）。开启后，系统将在发现目标插槽与其他护甲复用时发出警告或安全跳过，防止其他共用护甲被意外替换。
            </p>
            {antiPollution ? (
              <Chip
                label="防污染保护已开启: 将严格保护共用插槽"
                size="small"
                color="warning"
                variant="outlined"
              />
            ) : (
              <Chip
                label="标准替换模式: 替换目标护甲全部指定部件"
                size="small"
                variant="outlined"
              />
            )}
          </div>
        </div>

        {/* Slot Mapping & Pollution Preview Table */}
        <div className="mt-5 border-t border-hd2-border/60 pt-4">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-bold text-hd2-text">
              插槽映射与污染分析预览 ({mappings.length} 个插槽映射):
            </span>
            {loadingMappings && (
              <span className="text-xs text-hd2-yellow">正在分析插槽复用与污染关系...</span>
            )}
          </div>

          <div className="max-h-60 overflow-y-auto rounded border border-hd2-border/60 bg-black/50">
            <table className="w-full text-left text-xs text-hd2-text">
              <thead className="sticky top-0 bg-hd2-ink text-[0.6875rem] text-hd2-muted">
                <tr>
                  <th className="p-2">挂载点/插槽</th>
                  <th className="p-2">原Mod Unit哈希</th>
                  <th className="p-2">目标护甲Unit哈希</th>
                  <th className="p-2">污染检测与状态</th>
                </tr>
              </thead>
              <tbody>
                {mappings.map((m) => (
                  <tr key={m.slotKey} className="border-t border-hd2-border/40 hover:bg-white/5">
                    <td className="p-2 font-semibold text-hd2-yellow">{m.slotKey}</td>
                    <td className="p-2 font-mono text-hd2-muted">{m.sourceUnitHex}</td>
                    <td className="p-2 font-mono text-emerald-400">{m.targetUnitHex}</td>
                    <td className="p-2">
                      {m.isPolluted ? (
                        <span className="text-amber-400" title={`共用护甲: ${m.pollutedBy.join(", ")}`}>
                          ⚠️ 共用部件 (与 {m.pollutedBy[0]} 等复用)
                        </span>
                      ) : (
                        <span className="text-emerald-400">🟢 独立插槽 (无污染)</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* 4. Action Row & Execution */}
      <div className="rounded-lg border border-hd2-border bg-hd2-surface/80 p-5 shadow-lg">
        <div className="flex flex-col items-center justify-between gap-4 sm:flex-row">
          <div className="flex min-w-0 flex-1 flex-col">
            <div className="text-sm font-bold text-hd2-text">
              {busy ? "正在执行多部件迁移与打包..." : "准备就绪"}
            </div>
            <div className="truncate text-xs text-hd2-muted">
              {busy ? progressStage : "点击下方按钮将自动生成标准Arsenal Mod压缩包"}
            </div>
            {busy && (
              <div className="mt-2 w-full max-w-md">
                <LinearProgress
                  variant={progressPercent !== null ? "determinate" : "indeterminate"}
                  value={progressPercent ?? undefined}
                  color="warning"
                />
              </div>
            )}
          </div>

          <Button
            variant="contained"
            color="primary"
            size="large"
            disabled={busy || !modPath || !targetArmor}
            onClick={handleRunMigration}
            startIcon={<PlayArrowIcon />}
            sx={{ px: 4, py: 1.5, fontWeight: "bold" }}
          >
            开始迁移并生成Arsenal Mod包
          </Button>
        </div>

        {/* Success Summary Dialog / Box */}
        {summary && (
          <div className="mt-5 rounded border border-emerald-500/50 bg-emerald-950/30 p-4">
            <div className="flex items-center gap-2 text-sm font-bold text-emerald-400">
              <CheckCircleIcon fontSize="small" />
              <span>迁移完成！</span>
            </div>
            <div className="mt-2 grid grid-cols-1 gap-2 text-xs sm:grid-cols-3">
              <div>
                <span className="text-hd2-muted">已处理文件数: </span>
                <span className="font-semibold text-hd2-text">{summary.totalFilesCopied}</span>
              </div>
              <div>
                <span className="text-hd2-muted">总哈希替换次数: </span>
                <span className="font-semibold text-hd2-yellow">{summary.totalSubstitutions}</span>
              </div>
              <div>
                <span className="text-hd2-muted">已迁移部件数: </span>
                <span className="font-semibold text-emerald-400">{summary.componentsMigrated.length} 个</span>
              </div>
            </div>
            <p className="mt-3 break-all font-mono text-xs text-hd2-muted">
              输出文件: {summary.outputPath}
            </p>
            <div className="mt-3">
              <Button
                variant="outlined"
                size="small"
                onClick={() => revealItemInDir(summary.outputPath)}
              >
                在文件资源管理器中定位输出文件
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
