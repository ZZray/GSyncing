/**
 * Shared catalog of pre-configured game profiles. Used by both the first-run
 * wizard and the in-app "添加游戏" dialog. Paths follow the conventions in
 * Ludusavi's manifest, adapted for Windows users (Chinese region especially).
 *
 * `processName` is what `process_watch` matches against running processes —
 * exact Windows .exe name is fine, no path needed.
 */
export interface GamePreset {
  label: string;
  name: string;
  cover: string;
  paths: string[];
  process?: string;
  /** Coarse tag for UI grouping later. */
  category: "RPG" | "Action" | "Strategy" | "Roguelike" | "Sandbox" | "Other";
}

export const GAME_PRESETS: GamePreset[] = [
  // RPG
  {
    label: "原神（Genshin Impact）",
    name: "原神",
    cover: "原",
    paths: ["%USERPROFILE%/AppData/LocalLow/miHoYo/原神"],
    process: "YuanShen.exe",
    category: "RPG",
  },
  {
    label: "崩坏：星穹铁道",
    name: "星穹铁道",
    cover: "HSR",
    paths: ["%USERPROFILE%/AppData/LocalLow/miHoYo/崩坏：星穹铁道"],
    process: "StarRail.exe",
    category: "RPG",
  },
  {
    label: "巫师 3（GOG / Steam）",
    name: "巫师 3",
    cover: "W3",
    paths: ["%USERPROFILE%/Documents/The Witcher 3/gamesaves"],
    process: "witcher3.exe",
    category: "RPG",
  },
  {
    label: "赛博朋克 2077",
    name: "Cyberpunk 2077",
    cover: "2077",
    paths: ["%USERPROFILE%/Saved Games/CD Projekt Red/Cyberpunk 2077"],
    process: "Cyberpunk2077.exe",
    category: "RPG",
  },
  {
    label: "艾尔登法环（Elden Ring）",
    name: "Elden Ring",
    cover: "ER",
    paths: ["%APPDATA%/EldenRing"],
    process: "eldenring.exe",
    category: "RPG",
  },
  {
    label: "博德之门 3",
    name: "Baldur's Gate 3",
    cover: "BG3",
    paths: [
      "%LOCALAPPDATA%/Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames",
    ],
    process: "bg3.exe",
    category: "RPG",
  },
  {
    label: "上古卷轴 5：天际",
    name: "Skyrim",
    cover: "SK",
    paths: ["%USERPROFILE%/Documents/My Games/Skyrim Special Edition/Saves"],
    process: "SkyrimSE.exe",
    category: "RPG",
  },
  {
    label: "极乐迪斯科（Disco Elysium）",
    name: "Disco Elysium",
    cover: "DE",
    paths: ["%APPDATA%/DiscoElysium/Saves"],
    process: "disco.exe",
    category: "RPG",
  },

  // Action / Soulslike
  {
    label: "黑暗之魂 1",
    name: "Dark Souls",
    cover: "DS1",
    paths: ["%USERPROFILE%/Documents/NBGI/DarkSouls"],
    process: "DARKSOULS.exe",
    category: "Action",
  },
  {
    label: "黑暗之魂 2",
    name: "Dark Souls II",
    cover: "DS2",
    paths: ["%APPDATA%/DarkSoulsII"],
    process: "DarkSoulsII.exe",
    category: "Action",
  },
  {
    label: "黑暗之魂 3",
    name: "黑暗之魂 3",
    cover: "DS3",
    paths: ["%APPDATA%/DarkSoulsIII"],
    process: "DarkSoulsIII.exe",
    category: "Action",
  },
  {
    label: "只狼：影逝二度（Sekiro）",
    name: "Sekiro",
    cover: "SK",
    paths: ["%APPDATA%/Sekiro"],
    process: "sekiro.exe",
    category: "Action",
  },
  {
    label: "空洞骑士（Hollow Knight）",
    name: "Hollow Knight",
    cover: "HK",
    paths: ["%USERPROFILE%/AppData/LocalLow/Team Cherry/Hollow Knight"],
    process: "hollow_knight.exe",
    category: "Action",
  },

  // Strategy
  {
    label: "文明 VI",
    name: "Civilization VI",
    cover: "CIV",
    paths: ["%USERPROFILE%/Documents/My Games/Sid Meier's Civilization VI/Saves"],
    process: "CivilizationVI.exe",
    category: "Strategy",
  },
  {
    label: "异星工厂（Factorio）",
    name: "Factorio",
    cover: "FAC",
    paths: ["%APPDATA%/Factorio/saves"],
    process: "factorio.exe",
    category: "Strategy",
  },

  // Roguelike
  {
    label: "哈迪斯（Hades）",
    name: "Hades",
    cover: "H",
    paths: ["%USERPROFILE%/Saved Games/Hades"],
    process: "Hades.exe",
    category: "Roguelike",
  },

  // Sandbox / Survival
  {
    label: "我的世界（Minecraft Java）",
    name: "Minecraft",
    cover: "MC",
    paths: ["%APPDATA%/.minecraft/saves"],
    process: "javaw.exe",
    category: "Sandbox",
  },
  {
    label: "星露谷物语（Stardew Valley）",
    name: "Stardew Valley",
    cover: "SV",
    paths: ["%APPDATA%/StardewValley/Saves"],
    process: "Stardew Valley.exe",
    category: "Sandbox",
  },
  {
    label: "饥荒（Don't Starve Together）",
    name: "Don't Starve Together",
    cover: "DST",
    paths: ["%USERPROFILE%/Documents/Klei/DoNotStarveTogether"],
    process: "dontstarve_steam.exe",
    category: "Sandbox",
  },
  {
    label: "环世界（RimWorld）",
    name: "RimWorld",
    cover: "RW",
    paths: ["%APPDATA%/../LocalLow/Ludeon Studios/RimWorld by Ludeon Studios/Saves"],
    process: "RimWorldWin64.exe",
    category: "Sandbox",
  },
  {
    label: "泰拉瑞亚（Terraria）",
    name: "Terraria",
    cover: "TR",
    paths: ["%USERPROFILE%/Documents/My Games/Terraria"],
    process: "Terraria.exe",
    category: "Sandbox",
  },
  {
    label: "缺氧（Oxygen Not Included）",
    name: "Oxygen Not Included",
    cover: "ONI",
    paths: ["%USERPROFILE%/Documents/Klei/OxygenNotIncluded/save_files"],
    process: "OxygenNotIncluded.exe",
    category: "Sandbox",
  },

  // Other / Misc
  {
    label: "坎巴拉太空计划（KSP）",
    name: "KSP",
    cover: "KSP",
    paths: ["%PROGRAMFILES%/Steam/steamapps/common/Kerbal Space Program/saves"],
    process: "KSP_x64.exe",
    category: "Other",
  },

  // --- v1.6 expansion: more popular titles ---
  {
    label: "上古卷轴 4：湮灭",
    name: "Oblivion",
    cover: "OB",
    paths: ["%USERPROFILE%/Documents/My Games/Oblivion/Saves"],
    process: "OblivionRemastered.exe",
    category: "RPG",
  },
  {
    label: "辐射 4",
    name: "Fallout 4",
    cover: "FO4",
    paths: ["%USERPROFILE%/Documents/My Games/Fallout4/Saves"],
    process: "Fallout4.exe",
    category: "RPG",
  },
  {
    label: "女神异闻录 5 皇家版",
    name: "Persona 5 Royal",
    cover: "P5R",
    paths: ["%USERPROFILE%/Documents/My Games/P5R/Saves"],
    process: "P5R.exe",
    category: "RPG",
  },
  {
    label: "Red Dead Redemption 2",
    name: "RDR2",
    cover: "RDR2",
    paths: [
      "%USERPROFILE%/Documents/Rockstar Games/Red Dead Redemption 2/Profiles",
    ],
    process: "RDR2.exe",
    category: "Action",
  },
  {
    label: "GTA V",
    name: "GTA V",
    cover: "GTA",
    paths: ["%USERPROFILE%/Documents/Rockstar Games/GTA V/Profiles"],
    process: "GTA5.exe",
    category: "Action",
  },
  {
    label: "生化危机 2 重制版",
    name: "Resident Evil 2",
    cover: "RE2",
    paths: [
      "%USERPROFILE%/AppData/Local/CAPCOM/RESIDENT EVIL 2",
    ],
    process: "re2.exe",
    category: "Action",
  },
  {
    label: "死亡搁浅",
    name: "Death Stranding",
    cover: "DS",
    paths: [
      "%USERPROFILE%/AppData/Local/KojimaProductions/DeathStranding",
    ],
    process: "ds.exe",
    category: "Action",
  },
  {
    label: "Ori 系列（昏暗森林 / 鬼火之意志）",
    name: "Ori",
    cover: "OR",
    paths: ["%USERPROFILE%/AppData/Local/Ori and the Will of the Wisps"],
    process: "oriwotw.exe",
    category: "Action",
  },
  {
    label: "Phasmophobia",
    name: "Phasmophobia",
    cover: "PH",
    paths: [
      "%USERPROFILE%/AppData/LocalLow/Kinetic Games/Phasmophobia",
    ],
    process: "Phasmophobia.exe",
    category: "Action",
  },
  {
    label: "群星（Stellaris）",
    name: "Stellaris",
    cover: "ST",
    paths: ["%USERPROFILE%/Documents/Paradox Interactive/Stellaris/save games"],
    process: "stellaris.exe",
    category: "Strategy",
  },
  {
    label: "杀戮尖塔（Slay the Spire）",
    name: "Slay the Spire",
    cover: "STS",
    paths: [
      "%PROGRAMFILES%/Steam/steamapps/common/SlayTheSpire/saves",
    ],
    process: "SlayTheSpire.exe",
    category: "Roguelike",
  },
];
