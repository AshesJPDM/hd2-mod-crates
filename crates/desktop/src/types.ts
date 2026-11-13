export type {
  DetectedSource,
  EquipmentOption,
  MigrationMapping,
  MigrationSummary,
  MigrationVariant,
  MissingUnitPolicy,
  UnmatchedUnitPolicy,
  UnitRepatchSummary,
  UnifiedMigrateOptions,
  HeadArmorMergeOptions,
  HeadArmorMergeSummary,
  PatchMergeSummary,
} from "@hd2-mod-tools/migrator-ui";

import type {
  DetectedSource,
  EquipmentMappingPreview,
  EquipmentPartGraph,
  HeadArmorMergeOptions,
  UnifiedMigrateOptions,
  UnitRepatchOptions,
} from "@hd2-mod-tools/migrator-ui";

export type { EquipmentMappingPreview };

export interface PatchDescriptor {
  path: string;
  name: string;
  originalName: string | null;
  byteLength: number;
}

export interface DiscoveredPatchFile {
  name: string;
  path: string;
  relativePath: string;
  rootId: number;
  size: number;
  sourceRoot: string;
}

export interface InspectPatchResult {
  patch: PatchDescriptor;
  inspection: { sources: DetectedSource[] };
  equipmentGraph: EquipmentPartGraph;
}

export interface GameDataDiscovery {
  dataDir: string | null;
  candidates: string[];
}

export interface MigrateRequest {
  patchPaths: string[];
  dataDir: string;
  outputPath: string;
  options: UnifiedMigrateOptions;
}

export interface RepatchRequest {
  patchPaths: string[];
  dataDir: string;
  outputPath: string;
  options: UnitRepatchOptions;
}

export interface HeadArmorMergeRequest {
  armorPaths: string[];
  headPaths: string[] | null;
  outputPath: string;
  options: HeadArmorMergeOptions;
}

export interface PatchMergeRequest {
  inputPaths: string[][];
  outputName: string;
  outputPath: string;
}

export interface MigrationProgressEvent {
  completedBytes: number;
  targetName: string;
  targetHash: string;
  stage: string;
  kind: "outputProgress" | "targetStart" | "stage" | "targetFinish";
  totalBytes: number;
}

export interface AppUpdateMetadata {
  currentVersion: string;
  date: string;
  notes: string | null;
  target: string;
  version: string;
}

export interface ModularComponentInfo {
  id: string;
  name: string;
  patchFiles: string[];
  unitIds: string[];
}

export interface ModularModInspection {
  modName: string;
  author: string | null;
  version: string | null;
  detectedSourceArmor: string | null;
  detectedSourceHelmet: string | null;
  components: ModularComponentInfo[];
  hasManifest: boolean;
  hasMaterialPack: boolean;
  materialPatchFiles: string[];
}

export interface ModularSlotMapping {
  slotKey: string;
  sourceUnitId: number;
  sourceUnitHex: string;
  targetUnitId: number;
  targetUnitHex: string;
  isPolluted: boolean;
  pollutedBy: string[];
}

export interface ModularMigrateOptions {
  targetArmorName: string;
  targetHelmetName?: string | null;
  selectedComponents: string[];
  strategy: string;
  customUnitMappings: Record<string, string>;
  antiPollution: boolean;
  outputModName?: string | null;
}

export interface ModularMigrationSummary {
  outputPath: string;
  totalFilesCopied: number;
  totalSubstitutions: number;
  componentsMigrated: string[];
  warnings: string[];
}

export interface InspectModularModRequest {
  modPath: string;
  materialPackPath?: string | null;
}

export interface PreviewModularMappingsRequest {
  sourceArmor: string;
  targetArmor: string;
  sourceHelmet?: string | null;
  targetHelmet?: string | null;
  customOverrides: Record<string, string>;
}

export interface ModularMigrateRequest {
  modPath: string;
  materialPackPath?: string | null;
  outputPath: string;
  options: ModularMigrateOptions;
}
