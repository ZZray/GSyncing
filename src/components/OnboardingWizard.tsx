import { useState } from "react";
import {
  Modal,
  Steps,
  Form,
  Input,
  Select,
  Button,
  Switch,
  Space,
  Alert,
  App as AntdApp,
  Radio,
} from "antd";
import { CloudOutlined, CheckCircleOutlined } from "@ant-design/icons";
import { useAppStore } from "@/store";
import * as api from "@/api";
import { errMsg } from "@/api/err";
import { GAME_PRESETS as PRESET_CATALOG } from "@/data/gamePresets";
import type { BackendConfig, GameProfile } from "@/types";

interface Props {
  open: boolean;
  onClose: () => void;
}

const S3_PRESETS = [
  {
    label: "腾讯云 COS（上海）",
    endpoint: "https://cos.ap-shanghai.myqcloud.com",
    region: "ap-shanghai",
  },
  {
    label: "腾讯云 COS（广州）",
    endpoint: "https://cos.ap-guangzhou.myqcloud.com",
    region: "ap-guangzhou",
  },
  {
    label: "阿里云 OSS（杭州）",
    endpoint: "https://oss-cn-hangzhou.aliyuncs.com",
    region: "oss-cn-hangzhou",
  },
  {
    label: "AWS S3（us-east-1）",
    endpoint: "https://s3.us-east-1.amazonaws.com",
    region: "us-east-1",
  },
];

// Use the shared catalog defined in @/data/gamePresets.
const GAME_PRESETS = PRESET_CATALOG;

export default function OnboardingWizard({ open, onClose }: Props) {
  const [step, setStep] = useState(0);
  const [kind, setKind] = useState<"s3" | "webdav">("s3");
  const [backendForm] = Form.useForm();
  const [gameForm] = Form.useForm();
  const [testing, setTesting] = useState(false);
  const [creating, setCreating] = useState(false);
  const { message } = AntdApp.useApp();
  const saveBackend = useAppStore((s) => s.saveBackend);
  const saveGame = useAppStore((s) => s.saveGame);

  const reset = () => {
    setStep(0);
    setKind("s3");
    backendForm.resetFields();
    gameForm.resetFields();
  };

  const handleClose = () => {
    onClose();
    setTimeout(reset, 250);
  };

  const buildBackend = (
    values: Record<string, unknown>
  ): BackendConfig => {
    if (kind === "s3") {
      return {
        kind: "s3",
        name: (values.name as string) || "默认 S3",
        s3: {
          endpoint: values.endpoint as string,
          region: values.region as string,
          bucket: values.bucket as string,
          accessKeyId: values.accessKeyId as string,
          secretAccessKey: values.secretAccessKey as string,
          prefix: (values.prefix as string) || "gsyncing/",
          pathStyle: true,
        },
      };
    }
    return {
      kind: "webdav",
      name: (values.name as string) || "默认 WebDAV",
      webdav: {
        url: values.url as string,
        username: values.username as string,
        password: values.password as string,
        prefix: (values.prefix as string) || "gsyncing/",
      },
    };
  };

  const testAndSave = async () => {
    try {
      const values = await backendForm.validateFields();
      setTesting(true);
      const cfg = buildBackend(values);
      await api.testBackend(cfg);
      await saveBackend(cfg);
      message.success("后端验证通过，已保存");
      setStep(2);
    } catch (e: unknown) {
      if (e && typeof e === "object" && "errorFields" in e) return;
      message.error(`验证失败：${errMsg(e)}`);
    } finally {
      setTesting(false);
    }
  };

  const createFirstGame = async () => {
    try {
      const values = await gameForm.validateFields();
      setCreating(true);
      const preset = GAME_PRESETS.find((p) => p.label === values.preset);
      const game: GameProfile = {
        id: "",
        name: preset ? preset.name : (values.customName as string),
        cover: preset ? preset.cover : undefined,
        savePaths: preset
          ? [...preset.paths]
          : [values.customPath as string].filter(Boolean),
        include: ["**/*"],
        exclude: [],
        autoSync: values.autoSync ?? true,
        processName: preset?.process,
      };
      if (game.savePaths.length === 0) {
        message.error("请选择预设或填写存档路径");
        setCreating(false);
        return;
      }
      await saveGame(game);
      message.success(`已添加 ${game.name}`);
      handleClose();
    } catch (e: unknown) {
      if (e && typeof e === "object" && "errorFields" in e) return;
      message.error(`添加失败：${errMsg(e)}`);
    } finally {
      setCreating(false);
    }
  };

  return (
    <Modal
      open={open}
      onCancel={handleClose}
      footer={null}
      width={680}
      maskClosable={false}
      title="GSyncing 首次启动向导"
    >
      <Steps
        current={step}
        size="small"
        items={[
          { title: "选择云存储", icon: <CloudOutlined /> },
          { title: "填写凭据" },
          { title: "添加第一个游戏", icon: <CheckCircleOutlined /> },
        ]}
        style={{ marginBottom: 24 }}
      />

      {step === 0 && (
        <>
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
            message="GSyncing 会把游戏存档同步到你自己的云存储里。先选一个后端类型："
          />
          <Radio.Group
            value={kind}
            onChange={(e) => setKind(e.target.value)}
            style={{ display: "flex", flexDirection: "column", gap: 12 }}
          >
            <Radio value="s3" style={{ padding: 12, border: "1px solid #eef0f5", borderRadius: 8 }}>
              <b>S3 兼容</b> — 腾讯云 COS / 阿里云 OSS / AWS S3 / MinIO
              <div style={{ color: "#888", fontSize: 12, marginTop: 4 }}>
                按量计费，便宜稳定，推荐
              </div>
            </Radio>
            <Radio value="webdav" style={{ padding: 12, border: "1px solid #eef0f5", borderRadius: 8 }}>
              <b>WebDAV</b> — 坚果云 / Nextcloud / 自建
              <div style={{ color: "#888", fontSize: 12, marginTop: 4 }}>
                免费 / 已有云盘账户，可用
              </div>
            </Radio>
          </Radio.Group>
          <div style={{ marginTop: 20, textAlign: "right" }}>
            <Button type="primary" onClick={() => setStep(1)}>
              下一步
            </Button>
          </div>
        </>
      )}

      {step === 1 && (
        <>
          <Form form={backendForm} layout="vertical" preserve={false}>
            <Form.Item
              name="name"
              label="名称"
              initialValue={kind === "s3" ? "我的 COS" : "坚果云"}
            >
              <Input />
            </Form.Item>
            {kind === "s3" ? (
              <>
                <Form.Item label="预设">
                  <Select
                    allowClear
                    placeholder="选择一个预设填充 Endpoint / Region"
                    options={S3_PRESETS.map((p) => ({
                      label: p.label,
                      value: p.label,
                    }))}
                    onChange={(label?: string) => {
                      const p = S3_PRESETS.find((x) => x.label === label);
                      if (p)
                        backendForm.setFieldsValue({
                          endpoint: p.endpoint,
                          region: p.region,
                        });
                    }}
                  />
                </Form.Item>
                <Form.Item name="endpoint" label="Endpoint" rules={[{ required: true }]}>
                  <Input placeholder="https://cos.ap-shanghai.myqcloud.com" />
                </Form.Item>
                <Form.Item name="region" label="Region" rules={[{ required: true }]}>
                  <Input placeholder="ap-shanghai" />
                </Form.Item>
                <Form.Item name="bucket" label="Bucket" rules={[{ required: true }]}>
                  <Input placeholder="my-bucket-12345" />
                </Form.Item>
                <Form.Item name="accessKeyId" label="Access Key ID" rules={[{ required: true }]}>
                  <Input.Password />
                </Form.Item>
                <Form.Item name="secretAccessKey" label="Secret Access Key" rules={[{ required: true }]}>
                  <Input.Password />
                </Form.Item>
                <Form.Item name="prefix" label="Key 前缀" initialValue="gsyncing/">
                  <Input />
                </Form.Item>
              </>
            ) : (
              <>
                <Form.Item
                  name="url"
                  label="WebDAV URL"
                  rules={[{ required: true }]}
                  extra="坚果云：https://dav.jianguoyun.com/dav/"
                >
                  <Input placeholder="https://dav.jianguoyun.com/dav/" />
                </Form.Item>
                <Form.Item name="username" label="用户名" rules={[{ required: true }]}>
                  <Input />
                </Form.Item>
                <Form.Item name="password" label="密码 / 应用密码" rules={[{ required: true }]}>
                  <Input.Password />
                </Form.Item>
                <Form.Item name="prefix" label="子路径" initialValue="gsyncing/">
                  <Input />
                </Form.Item>
              </>
            )}
          </Form>
          <Space style={{ width: "100%", justifyContent: "space-between" }}>
            <Button onClick={() => setStep(0)}>上一步</Button>
            <Button type="primary" loading={testing} onClick={testAndSave}>
              测试连接并保存
            </Button>
          </Space>
        </>
      )}

      {step === 2 && (
        <>
          <Alert
            type="success"
            showIcon
            style={{ marginBottom: 16 }}
            message="云存储已配置好。现在添加你的第一个游戏开始同步："
          />
          <Form form={gameForm} layout="vertical">
            <Form.Item name="preset" label="从预设选择">
              <Select
                allowClear
                placeholder="或下方手动填写"
                options={GAME_PRESETS.map((p) => ({
                  label: p.label,
                  value: p.label,
                }))}
              />
            </Form.Item>
            <Form.Item
              shouldUpdate={(prev, cur) => prev.preset !== cur.preset}
              noStyle
            >
              {({ getFieldValue }) =>
                !getFieldValue("preset") ? (
                  <>
                    <Form.Item name="customName" label="游戏名称">
                      <Input placeholder="例如：我的某游戏" />
                    </Form.Item>
                    <Form.Item name="customPath" label="存档路径">
                      <Input placeholder="支持 %USERPROFILE% / %APPDATA% / %LOCALAPPDATA%" />
                    </Form.Item>
                  </>
                ) : null
              }
            </Form.Item>
            <Form.Item name="autoSync" label="自动同步" valuePropName="checked" initialValue={true}>
              <Switch />
            </Form.Item>
          </Form>
          <Space style={{ width: "100%", justifyContent: "space-between" }}>
            <Button onClick={() => setStep(1)}>上一步</Button>
            <Space>
              <Button onClick={handleClose}>稍后再说</Button>
              <Button type="primary" loading={creating} onClick={createFirstGame}>
                完成
              </Button>
            </Space>
          </Space>
        </>
      )}
    </Modal>
  );
}
