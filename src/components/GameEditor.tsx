import { useEffect, useState } from "react";
import {
  Modal,
  Form,
  Input,
  Switch,
  Button,
  Space,
  Select,
  App as AntdApp,
} from "antd";
import {
  FolderOpenOutlined,
  PlusOutlined,
  MinusOutlined,
  CheckCircleOutlined,
} from "@ant-design/icons";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "@/store";
import { errMsg } from "@/api/err";
import { validateGamePaths } from "@/api";
import { GAME_PRESETS as PRESET_CATALOG } from "@/data/gamePresets";
import type { GameProfile, GameSizeInfo } from "@/types";

interface Props {
  open: boolean;
  initial: GameProfile | null;
  onClose: () => void;
}

interface FormValues {
  name: string;
  cover?: string;
  savePaths: string[];
  include: string[];
  exclude: string[];
  remotePrefix?: string;
  autoSync: boolean;
  processName?: string;
  backend?: string;
  category?: string;
}

// Adapter from the shared catalog into the form's GameProfile shape.
const PRESETS: Array<{ label: string; category: string; value: GameProfile }> =
  PRESET_CATALOG.map((p) => ({
    label: p.label,
    category: p.category,
    value: {
      id: "",
      name: p.name,
      cover: p.cover,
      savePaths: [...p.paths],
      include: ["**/*"],
      exclude: [],
      autoSync: true,
      processName: p.process,
      category: p.category,
    },
  }));

// Group presets by category for the Select.OptGroup dropdown.
const CATEGORY_LABELS: Record<string, string> = {
  RPG: "🎭 RPG",
  Action: "⚔️ 动作 / 魂系",
  Strategy: "🎯 策略",
  Roguelike: "🎲 Roguelike",
  Sandbox: "🌍 沙盒 / 生存",
  Other: "🎮 其它",
};
const PRESET_GROUPS = Object.entries(
  PRESETS.reduce<Record<string, typeof PRESETS>>((acc, p) => {
    (acc[p.category] ??= []).push(p);
    return acc;
  }, {})
).map(([cat, items]) => ({
  label: CATEGORY_LABELS[cat] ?? cat,
  options: items.map((i) => ({ label: i.label, value: i.label })),
}));

export default function GameEditor({ open, initial, onClose }: Props) {
  const [form] = Form.useForm<FormValues>();
  const saveGame = useAppStore((s) => s.saveGame);
  const backends = useAppStore((s) => s.backends);
  const { message } = AntdApp.useApp();
  const [validating, setValidating] = useState(false);
  const [validateResult, setValidateResult] = useState<{
    ok: boolean;
    text: string;
  } | null>(null);

  const onValidate = async () => {
    try {
      const values = await form.validateFields();
      setValidating(true);
      setValidateResult(null);
      const game: GameProfile = {
        id: initial?.id ?? "preview",
        name: values.name.trim() || "preview",
        savePaths: values.savePaths.filter((p) => p && p.trim()),
        include:
          values.include.filter((p) => p && p.trim()).length > 0
            ? values.include.filter((p) => p && p.trim())
            : ["**/*"],
        exclude: values.exclude.filter((p) => p && p.trim()),
        autoSync: false,
      };
      if (game.savePaths.length === 0) {
        setValidateResult({ ok: false, text: "请至少填一个路径" });
        return;
      }
      const info: GameSizeInfo = await validateGamePaths(game);
      if (info.fileCount === 0) {
        setValidateResult({
          ok: false,
          text: "路径有效但扫不到任何文件（检查 include / exclude glob 或路径是否存在）",
        });
      } else {
        const mb = info.totalBytes / 1024 / 1024;
        setValidateResult({
          ok: true,
          text: `扫到 ${info.fileCount} 个文件，共 ${
            mb < 1
              ? `${(info.totalBytes / 1024).toFixed(1)} KB`
              : `${mb.toFixed(2)} MB`
          }`,
        });
      }
    } catch (e: unknown) {
      if (e && typeof e === "object" && "errorFields" in e) return;
      setValidateResult({ ok: false, text: `验证失败：${errMsg(e)}` });
    } finally {
      setValidating(false);
    }
  };

  useEffect(() => {
    if (open) {
      form.setFieldsValue({
        name: initial?.name ?? "",
        cover: initial?.cover ?? "",
        savePaths: initial?.savePaths ?? [""],
        include: initial?.include ?? ["**/*"],
        exclude: initial?.exclude ?? [],
        remotePrefix: initial?.remotePrefix ?? "",
        autoSync: initial?.autoSync ?? true,
        processName: initial?.processName ?? "",
        backend: initial?.backend ?? undefined,
        category: initial?.category ?? undefined,
      });
    }
  }, [open, initial, form]);

  const handlePickFolder = async (idx: number) => {
    const result = await openDialog({ directory: true, multiple: false });
    if (typeof result === "string") {
      const current = form.getFieldValue("savePaths") as string[];
      current[idx] = result;
      form.setFieldsValue({ savePaths: [...current] });
    }
  };

  const onApplyPreset = (presetName: string) => {
    const preset = PRESETS.find((p) => p.label === presetName);
    if (!preset) return;
    form.setFieldsValue({
      name: preset.value.name,
      cover: preset.value.cover,
      savePaths: [...preset.value.savePaths],
      include: [...preset.value.include],
      exclude: [...preset.value.exclude],
      autoSync: preset.value.autoSync,
      processName: preset.value.processName,
      category: preset.value.category,
    });
  };

  const onOk = async () => {
    try {
      const values = await form.validateFields();
      const game: GameProfile = {
        id: initial?.id ?? "",
        name: values.name.trim(),
        cover: values.cover?.trim() || undefined,
        savePaths: values.savePaths.filter((p) => p && p.trim()),
        include:
          values.include.filter((p) => p && p.trim()).length > 0
            ? values.include.filter((p) => p && p.trim())
            : ["**/*"],
        exclude: values.exclude.filter((p) => p && p.trim()),
        remotePrefix: values.remotePrefix?.trim() || undefined,
        autoSync: values.autoSync,
        processName: values.processName?.trim() || undefined,
        backend: values.backend?.trim() || undefined,
        category: values.category?.trim() || undefined,
      };
      if (game.savePaths.length === 0) {
        message.error("至少需要一个存档路径");
        return;
      }
      await saveGame(game);
      message.success("已保存");
      onClose();
    } catch (e: unknown) {
      if (e && typeof e === "object" && "errorFields" in e) return;
      message.error(`保存失败：${errMsg(e)}`);
    }
  };

  return (
    <Modal
      title={initial ? `编辑：${initial.name}` : "添加游戏"}
      open={open}
      onCancel={onClose}
      onOk={onOk}
      width={680}
      okText="保存"
      cancelText="取消"
    >
      {!initial && (
        <div style={{ marginBottom: 14 }}>
          <Select
            placeholder="从预设导入...（按品类分组，23 款常见游戏）"
            style={{ width: "100%" }}
            onChange={onApplyPreset}
            options={PRESET_GROUPS}
            allowClear
            showSearch
            optionFilterProp="label"
          />
        </div>
      )}
      <Form form={form} layout="vertical">
        <Form.Item
          name="name"
          label="游戏名称"
          rules={[{ required: true, message: "请输入游戏名称" }]}
        >
          <Input placeholder="例如：原神" />
        </Form.Item>
        <Form.Item name="cover" label="封面文字 (2-3 字符)">
          <Input maxLength={4} placeholder="如：原 / DS3 / W3" />
        </Form.Item>

        <Form.List name="savePaths">
          {(fields, { add, remove }) => (
            <div>
              <div
                style={{
                  marginBottom: 6,
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                }}
              >
                <span>
                  存档路径
                  <span style={{ color: "#999", marginLeft: 8 }}>
                    支持环境变量 %USERPROFILE% / %APPDATA% / %LOCALAPPDATA%
                  </span>
                </span>
                <Button
                  size="small"
                  icon={<CheckCircleOutlined />}
                  onClick={onValidate}
                  loading={validating}
                >
                  验证路径
                </Button>
              </div>
              {validateResult && (
                <div
                  style={{
                    marginBottom: 8,
                    padding: "6px 10px",
                    borderRadius: 6,
                    fontSize: 12,
                    background: validateResult.ok ? "#f6ffed" : "#fff7e6",
                    border: `1px solid ${
                      validateResult.ok ? "#b7eb8f" : "#ffd591"
                    }`,
                    color: validateResult.ok ? "#389e0d" : "#d46b08",
                  }}
                >
                  {validateResult.ok ? "✓" : "⚠"} {validateResult.text}
                </div>
              )}
              {fields.map(({ key, name, ...rest }) => (
                <Space key={key} style={{ display: "flex", marginBottom: 6 }}>
                  <Form.Item {...rest} name={name} noStyle>
                    <Input style={{ width: 460 }} />
                  </Form.Item>
                  <Button
                    icon={<FolderOpenOutlined />}
                    onClick={() => handlePickFolder(name)}
                  />
                  <Button
                    icon={<MinusOutlined />}
                    onClick={() => remove(name)}
                    danger
                  />
                </Space>
              ))}
              <Button
                type="dashed"
                icon={<PlusOutlined />}
                onClick={() => add("")}
                block
              >
                添加存档路径
              </Button>
            </div>
          )}
        </Form.List>

        <Form.List name="include">
          {(fields, { add, remove }) => (
            <div style={{ marginTop: 16 }}>
              <div style={{ marginBottom: 6 }}>
                包含 glob (留空表示全部)
              </div>
              {fields.map(({ key, name, ...rest }) => (
                <Space key={key} style={{ display: "flex", marginBottom: 6 }}>
                  <Form.Item {...rest} name={name} noStyle>
                    <Input style={{ width: 460 }} placeholder="例如 **/*.sav" />
                  </Form.Item>
                  <Button
                    icon={<MinusOutlined />}
                    onClick={() => remove(name)}
                    danger
                  />
                </Space>
              ))}
              <Button
                type="dashed"
                icon={<PlusOutlined />}
                onClick={() => add("")}
                block
              >
                添加 include
              </Button>
            </div>
          )}
        </Form.List>

        <Form.List name="exclude">
          {(fields, { add, remove }) => (
            <div style={{ marginTop: 16 }}>
              <div style={{ marginBottom: 6 }}>排除 glob</div>
              {fields.map(({ key, name, ...rest }) => (
                <Space key={key} style={{ display: "flex", marginBottom: 6 }}>
                  <Form.Item {...rest} name={name} noStyle>
                    <Input
                      style={{ width: 460 }}
                      placeholder="例如 **/cache/**"
                    />
                  </Form.Item>
                  <Button
                    icon={<MinusOutlined />}
                    onClick={() => remove(name)}
                    danger
                  />
                </Space>
              ))}
              <Button
                type="dashed"
                icon={<PlusOutlined />}
                onClick={() => add("")}
                block
              >
                添加 exclude
              </Button>
            </div>
          )}
        </Form.List>

        <Form.Item
          name="remotePrefix"
          label="远端前缀（留空使用游戏 id）"
          style={{ marginTop: 16 }}
        >
          <Input placeholder="例如 saves/genshin" />
        </Form.Item>
        <Form.Item
          name="processName"
          label="关联进程名（用于退出游戏后自动同步）"
        >
          <Input placeholder="例如 YuanShen.exe" />
        </Form.Item>
        <Form.Item
          name="backend"
          label="云存储后端（留空 = 使用默认）"
        >
          <Select
            allowClear
            placeholder="使用默认后端"
            options={backends.map((b) => ({ label: b.name, value: b.name }))}
          />
        </Form.Item>
        <Form.Item
          name="category"
          label="品类（用于卡片着色和分组，可选）"
        >
          <Select
            allowClear
            placeholder="自动从预设填充"
            options={[
              { label: "🎭 RPG", value: "RPG" },
              { label: "⚔️ 动作 / 魂系", value: "Action" },
              { label: "🎯 策略", value: "Strategy" },
              { label: "🎲 Roguelike", value: "Roguelike" },
              { label: "🌍 沙盒 / 生存", value: "Sandbox" },
              { label: "🎮 其它", value: "Other" },
            ]}
          />
        </Form.Item>
        <Form.Item name="autoSync" label="自动同步" valuePropName="checked">
          <Switch />
        </Form.Item>
      </Form>
    </Modal>
  );
}
