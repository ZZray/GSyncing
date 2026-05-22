import { useEffect, useState } from "react";
import {
  Card,
  Button,
  Form,
  Input,
  Switch,
  Select,
  Space,
  Tag,
  Popconfirm,
  App as AntdApp,
  Tabs,
  InputNumber,
  Divider,
  Radio,
} from "antd";
import {
  DeleteOutlined,
  EditOutlined,
  PlusOutlined,
  ExportOutlined,
  ImportOutlined,
} from "@ant-design/icons";
import { useAppStore } from "@/store";
import * as api from "@/api";
import { errMsg } from "@/api/err";
import type { BackendConfig, AppSettings } from "@/types";
import { save as saveDialog, open as openDialog } from "@tauri-apps/plugin-dialog";
import { Modal as AntdModal } from "antd";

export default function Settings() {
  return (
    <>
      <div className="page-header">
        <div>
          <h2 className="page-title">云存储设置</h2>
          <div className="page-subtitle">配置 S3 / WebDAV 后端，调整同步行为</div>
        </div>
        <ConfigPortabilityButtons />
      </div>
      <Tabs
        items={[
          { key: "backends", label: "云存储后端", children: <BackendSection /> },
          { key: "general", label: "通用设置", children: <GeneralSection /> },
        ]}
      />
    </>
  );
}

function ConfigPortabilityButtons() {
  const { message } = AntdApp.useApp();
  const loadAll = useAppStore((s) => s.loadAll);

  const handleExport = () => {
    AntdModal.confirm({
      title: "导出配置",
      content: (
        <div>
          <p>选择是否包含敏感凭据（Access Key / 密码）：</p>
          <ul style={{ color: "#555" }}>
            <li>
              <b>不包含</b>（推荐）：导出后的文件可以安全分享；导入到新机器后需要手动重新填密钥
            </li>
            <li>
              <b>包含</b>：导出的文件含明文密钥，仅在你完全信任目标设备时使用
            </li>
          </ul>
        </div>
      ),
      okText: "包含凭据",
      cancelText: "不包含凭据",
      onOk: async () => doExport(true),
      onCancel: () => doExport(false),
    });
  };

  const doExport = async (includeSecrets: boolean) => {
    const dst = await saveDialog({
      defaultPath: `gsyncing-config-${new Date()
        .toISOString()
        .slice(0, 10)}.json`,
      title: "导出 GSyncing 配置",
    });
    if (!dst) return;
    try {
      const written = await api.exportConfig(dst, includeSecrets);
      message.success(`已导出到 ${written}`);
    } catch (e: unknown) {
      message.error(`导出失败：${errMsg(e)}`);
    }
  };

  const handleImport = () => {
    AntdModal.confirm({
      title: "导入配置",
      content: (
        <div>
          <p>选择合并方式：</p>
          <ul style={{ color: "#555" }}>
            <li>
              <b>合并</b>：保留现有 + 加入导入的；同 id 游戏 / 同名后端被覆盖
            </li>
            <li>
              <b>替换</b>：清空当前配置，全部替换为导入文件（高风险）
            </li>
          </ul>
        </div>
      ),
      okText: "合并",
      cancelText: "替换",
      onOk: async () => doImport("merge"),
      onCancel: () => doImport("replace"),
    });
  };

  const doImport = async (mode: "merge" | "replace") => {
    const src = await openDialog({
      multiple: false,
      filters: [{ name: "JSON", extensions: ["json"] }],
      title: "选择 GSyncing 配置文件",
    });
    if (typeof src !== "string") return;
    try {
      await api.importConfig(src, mode);
      await loadAll();
      message.success(`已${mode === "merge" ? "合并" : "替换"}导入配置`);
    } catch (e: unknown) {
      message.error(`导入失败：${errMsg(e)}`);
    }
  };

  return (
    <Space>
      <Button icon={<ExportOutlined />} onClick={handleExport}>
        导出配置
      </Button>
      <Button icon={<ImportOutlined />} onClick={handleImport}>
        导入配置
      </Button>
    </Space>
  );
}

function BackendSection() {
  const backends = useAppStore((s) => s.backends);
  const deleteBackend = useAppStore((s) => s.deleteBackend);
  const [editing, setEditing] = useState<BackendConfig | null>(null);
  const [creating, setCreating] = useState(false);

  return (
    <div>
      <div style={{ marginBottom: 14 }}>
        <Button
          type="primary"
          icon={<PlusOutlined />}
          onClick={() => setCreating(true)}
        >
          添加后端
        </Button>
      </div>
      <Space direction="vertical" style={{ width: "100%" }} size="middle">
        {backends.length === 0 && (
          <Card>
            <div style={{ color: "#999", textAlign: "center", padding: 20 }}>
              还没有配置任何云存储后端
            </div>
          </Card>
        )}
        {backends.map((b) => (
          <Card
            key={b.name}
            size="small"
            title={
              <Space>
                <Tag color={b.kind === "s3" ? "blue" : "purple"}>
                  {b.kind === "s3" ? "S3" : "WebDAV"}
                </Tag>
                <span>{b.name}</span>
              </Space>
            }
            extra={
              <Space>
                <Button
                  size="small"
                  icon={<EditOutlined />}
                  onClick={() => setEditing(b)}
                >
                  编辑
                </Button>
                <Popconfirm
                  title={`删除后端 ${b.name}？`}
                  onConfirm={() => deleteBackend(b.name)}
                >
                  <Button size="small" danger icon={<DeleteOutlined />}>
                    删除
                  </Button>
                </Popconfirm>
              </Space>
            }
          >
            <BackendSummary backend={b} />
          </Card>
        ))}
      </Space>

      <BackendEditor
        open={creating || editing !== null}
        initial={editing}
        onClose={() => {
          setCreating(false);
          setEditing(null);
        }}
      />
    </div>
  );
}

function BackendSummary({ backend }: { backend: BackendConfig }) {
  if (backend.kind === "s3") {
    return (
      <div style={{ color: "#555", fontSize: 13 }}>
        Endpoint: {backend.s3.endpoint} · Region: {backend.s3.region} ·
        Bucket: <b>{backend.s3.bucket}</b> · Prefix: {backend.s3.prefix || "/"}
      </div>
    );
  }
  return (
    <div style={{ color: "#555", fontSize: 13 }}>
      URL: {backend.webdav.url} · Prefix: {backend.webdav.prefix || "/"} ·
      User: {backend.webdav.username}
    </div>
  );
}

interface EditorProps {
  open: boolean;
  initial: BackendConfig | null;
  onClose: () => void;
}

function BackendEditor({ open, initial, onClose }: EditorProps) {
  const [form] = Form.useForm();
  const [kind, setKind] = useState<"s3" | "webdav">("s3");
  const saveBackend = useAppStore((s) => s.saveBackend);
  const { message } = AntdApp.useApp();
  const [testing, setTesting] = useState(false);

  useEffect(() => {
    if (!open) return;
    if (initial) {
      setKind(initial.kind);
      if (initial.kind === "s3") {
        form.setFieldsValue({
          name: initial.name,
          kind: "s3",
          ...initial.s3,
        });
      } else {
        form.setFieldsValue({
          name: initial.name,
          kind: "webdav",
          ...initial.webdav,
        });
      }
    } else {
      form.resetFields();
      setKind("s3");
      form.setFieldsValue({
        kind: "s3",
        pathStyle: true,
        region: "us-east-1",
        prefix: "gsyncing/",
      });
    }
  }, [open, initial, form]);

  const buildBackend = (values: Record<string, unknown>): BackendConfig => {
    if (kind === "s3") {
      return {
        kind: "s3",
        name: values.name as string,
        s3: {
          endpoint: values.endpoint as string,
          region: values.region as string,
          bucket: values.bucket as string,
          accessKeyId: values.accessKeyId as string,
          secretAccessKey: values.secretAccessKey as string,
          prefix: (values.prefix as string) ?? "",
          pathStyle: (values.pathStyle as boolean) ?? true,
        },
      };
    }
    return {
      kind: "webdav",
      name: values.name as string,
      webdav: {
        url: values.url as string,
        username: values.username as string,
        password: values.password as string,
        prefix: (values.prefix as string) ?? "",
      },
    };
  };

  const onTest = async () => {
    try {
      const values = await form.validateFields();
      setTesting(true);
      const result = await api.testBackend(buildBackend(values));
      message.success(`连通性测试通过：${result}`);
    } catch (e: unknown) {
      if (e && typeof e === "object" && "errorFields" in e) return;
      message.error(`连接失败：${errMsg(e)}`);
    } finally {
      setTesting(false);
    }
  };

  const onSave = async () => {
    try {
      const values = await form.validateFields();
      await saveBackend(buildBackend(values));
      message.success("已保存");
      onClose();
    } catch (e: unknown) {
      if (e && typeof e === "object" && "errorFields" in e) return;
      message.error(`保存失败：${errMsg(e)}`);
    }
  };

  if (!open) return null;

  return (
    <Card
      title={initial ? `编辑：${initial.name}` : "新建后端"}
      style={{ marginTop: 16 }}
      extra={
        <Space>
          <Button onClick={onClose}>取消</Button>
          <Button loading={testing} onClick={onTest}>
            连通性测试
          </Button>
          <Button type="primary" onClick={onSave}>
            保存
          </Button>
        </Space>
      }
    >
      <Form form={form} layout="vertical">
        <Form.Item
          label="名称"
          name="name"
          rules={[{ required: true, message: "请输入名称" }]}
        >
          <Input placeholder="例如：腾讯云 COS 主仓 / 坚果云" />
        </Form.Item>
        <Form.Item label="类型" name="kind">
          <Radio.Group
            onChange={(e) => setKind(e.target.value)}
            value={kind}
            options={[
              { label: "S3 (兼容 COS / OSS / AWS)", value: "s3" },
              { label: "WebDAV", value: "webdav" },
            ]}
          />
        </Form.Item>
        <Divider />
        {kind === "s3" ? <S3Fields /> : <WebDAVFields />}
      </Form>
    </Card>
  );
}

function S3Fields() {
  const PRESETS = [
    {
      label: "腾讯云 COS（上海）",
      value: "https://cos.ap-shanghai.myqcloud.com",
      region: "ap-shanghai",
    },
    {
      label: "腾讯云 COS（广州）",
      value: "https://cos.ap-guangzhou.myqcloud.com",
      region: "ap-guangzhou",
    },
    {
      label: "阿里云 OSS（杭州）",
      value: "https://oss-cn-hangzhou.aliyuncs.com",
      region: "oss-cn-hangzhou",
    },
    {
      label: "阿里云 OSS（深圳）",
      value: "https://oss-cn-shenzhen.aliyuncs.com",
      region: "oss-cn-shenzhen",
    },
    {
      label: "AWS S3（us-east-1）",
      value: "https://s3.us-east-1.amazonaws.com",
      region: "us-east-1",
    },
  ];
  const form = Form.useFormInstance();

  return (
    <>
      <Form.Item label="预设" extra="选择后会自动填充 Endpoint / Region">
        <Select
          allowClear
          placeholder="选择一个预设..."
          options={PRESETS.map((p) => ({ label: p.label, value: p.label }))}
          onChange={(label?: string) => {
            const p = PRESETS.find((x) => x.label === label);
            if (p) {
              form.setFieldsValue({ endpoint: p.value, region: p.region });
            }
          }}
        />
      </Form.Item>
      <Form.Item
        label="Endpoint"
        name="endpoint"
        rules={[{ required: true, message: "请输入 Endpoint" }]}
      >
        <Input placeholder="https://cos.ap-shanghai.myqcloud.com" />
      </Form.Item>
      <Form.Item
        label="Region"
        name="region"
        rules={[{ required: true, message: "请输入 Region" }]}
      >
        <Input placeholder="ap-shanghai" />
      </Form.Item>
      <Form.Item
        label="Bucket"
        name="bucket"
        rules={[{ required: true, message: "请输入 Bucket" }]}
      >
        <Input placeholder="my-bucket-12345" />
      </Form.Item>
      <Form.Item
        label="Access Key ID"
        name="accessKeyId"
        rules={[{ required: true }]}
      >
        <Input.Password placeholder="AKID..." visibilityToggle />
      </Form.Item>
      <Form.Item
        label="Secret Access Key"
        name="secretAccessKey"
        rules={[{ required: true }]}
      >
        <Input.Password placeholder="..." visibilityToggle />
      </Form.Item>
      <Form.Item label="前缀（key prefix）" name="prefix">
        <Input placeholder="gsyncing/" />
      </Form.Item>
      <Form.Item
        label="Path-style 寻址"
        name="pathStyle"
        valuePropName="checked"
        extra="腾讯云 / 阿里云 / MinIO 一般保持开启"
      >
        <Switch />
      </Form.Item>
    </>
  );
}

function WebDAVFields() {
  return (
    <>
      <Form.Item
        label="WebDAV URL"
        name="url"
        rules={[{ required: true, message: "请输入 URL" }]}
        extra="坚果云：https://dav.jianguoyun.com/dav/"
      >
        <Input placeholder="https://dav.jianguoyun.com/dav/" />
      </Form.Item>
      <Form.Item
        label="用户名"
        name="username"
        rules={[{ required: true }]}
      >
        <Input />
      </Form.Item>
      <Form.Item
        label="密码 / 应用密码"
        name="password"
        rules={[{ required: true }]}
      >
        <Input.Password visibilityToggle />
      </Form.Item>
      <Form.Item label="子路径" name="prefix">
        <Input placeholder="gsyncing/" />
      </Form.Item>
    </>
  );
}

const BANDWIDTH_PRESETS: Array<{ label: string; bytes: number }> = [
  { label: "不限", bytes: 0 },
  { label: "512 KB/s", bytes: 512 * 1024 },
  { label: "1 MB/s", bytes: 1024 * 1024 },
  { label: "2 MB/s", bytes: 2 * 1024 * 1024 },
  { label: "5 MB/s", bytes: 5 * 1024 * 1024 },
  { label: "10 MB/s", bytes: 10 * 1024 * 1024 },
  { label: "50 MB/s", bytes: 50 * 1024 * 1024 },
];

function BandwidthSelector(props: {
  value?: number;
  onChange?: (v: number) => void;
}) {
  const { value = 0, onChange } = props;
  const displayMb = value > 0 ? (value / 1024 / 1024).toFixed(2) : "0";
  return (
    <Space wrap>
      {BANDWIDTH_PRESETS.map((p) => (
        <Button
          key={p.bytes}
          size="small"
          type={value === p.bytes ? "primary" : "default"}
          onClick={() => onChange?.(p.bytes)}
        >
          {p.label}
        </Button>
      ))}
      <span style={{ color: "#888", fontSize: 12 }}>
        当前：{value === 0 ? "不限速" : `${displayMb} MB/s`}
      </span>
    </Space>
  );
}

function GeneralSection() {
  const settings = useAppStore((s) => s.settings);
  const backends = useAppStore((s) => s.backends);
  const saveSettings = useAppStore((s) => s.saveSettings);
  const [form] = Form.useForm<AppSettings>();
  const { message } = AntdApp.useApp();

  useEffect(() => {
    if (settings) form.setFieldsValue(settings);
  }, [settings, form]);

  if (!settings) return null;

  const onSave = async () => {
    const values = await form.validateFields();
    await saveSettings(values);
    message.success("已保存");
  };

  return (
    <Card>
      <Form form={form} layout="vertical" style={{ maxWidth: 540 }}>
        <Form.Item label="默认后端" name="defaultBackend">
          <Select
            allowClear
            placeholder="选择默认后端"
            options={backends.map((b) => ({ label: b.name, value: b.name }))}
          />
        </Form.Item>
        <Form.Item
          label="自动同步间隔（秒）"
          name="autoSyncIntervalSec"
          extra="0 表示不开启周期同步"
        >
          <InputNumber min={0} max={86400} style={{ width: 200 }} />
        </Form.Item>
        <Form.Item
          label="启用文件监控"
          name="enableFileWatcher"
          valuePropName="checked"
          extra="本地存档变化后延迟数秒触发同步"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="退出游戏后自动同步（仅 Windows）"
          name="enableExitSync"
          valuePropName="checked"
          extra="检测到游戏进程退出后自动推送一次存档"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="同步前显示预览"
          name="alwaysPreview"
          valuePropName="checked"
          extra="手动同步时先弹窗显示将要做的事，确认后再执行（FreeFileSync 风格）"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="关闭按钮最小化到托盘"
          name="closeToTray"
          valuePropName="checked"
          extra="关闭窗口后程序在后台继续运行（文件监控 / 自动同步 / 进程检测不会中断）。从托盘菜单退出。"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="带宽限制"
          name="maxBytesPerSec"
          extra="后台同步时限速避免占满上行影响游戏。0 = 不限速"
          getValueFromEvent={(v) => v}
        >
          <BandwidthSelector />
        </Form.Item>
        <Form.Item
          label="同步完成系统通知"
          name="notifyOnComplete"
          valuePropName="checked"
          extra="最小化到托盘时也能看到同步完成 / 失败的桌面通知"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="启动时自动打开开发者工具"
          name="autoOpenDevtools"
          valuePropName="checked"
          extra="白屏诊断用。一切正常后建议关闭（关闭后下次启动生效）"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="界面主题"
          name="theme"
          extra="深色模式即时生效，无需重启"
        >
          <Select
            style={{ width: 200 }}
            options={[
              { label: "🌞 浅色", value: "light" },
              { label: "🌙 深色", value: "dark" },
              { label: "🖥️ 跟随系统", value: "auto" },
            ]}
          />
        </Form.Item>
        <Form.Item
          label="启动时自动检查更新"
          name="autoCheckUpdates"
          valuePropName="checked"
          extra="启动后静默检查；有新版本会弹出对话框让你选是否下载"
        >
          <Switch />
        </Form.Item>
        <Form.Item
          label="冲突策略"
          name="conflictPolicy"
          extra="保留双方最安全（FreeFileSync 默认）；更新者胜适合单人多机；其它策略可能丢数据"
        >
          <Select
            options={[
              { label: "保留双方（推荐 · 重命名败方）", value: "rename-both" },
              { label: "更新者胜", value: "newer-wins" },
              { label: "本地优先", value: "local-wins" },
              { label: "远端优先", value: "remote-wins" },
              { label: "弹窗确认（未实现，降级 newer-wins）", value: "ask" },
            ]}
          />
        </Form.Item>
        <Form.Item
          label="并发传输数"
          name="maxConcurrency"
          extra="同时上传/下载的最大文件数。家用网络建议 4-8"
        >
          <InputNumber min={1} max={32} style={{ width: 200 }} />
        </Form.Item>
        <Form.Item
          label="保留历史版本数"
          name="versionsToKeep"
          extra="远端每个文件被覆盖前自动归档，最多保留 N 个旧版本。0 关闭"
        >
          <InputNumber min={0} max={50} style={{ width: 200 }} />
        </Form.Item>
        <Button type="primary" onClick={onSave}>
          保存
        </Button>
      </Form>
    </Card>
  );
}
