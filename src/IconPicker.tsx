/**
 * Popover for picking an icon override for a card or a group.
 *
 * Curated set of lucide-react icons (~80) that cover the common cases. The
 * picked name is stored on `node.icon` / `group.icon`; the rendering layer
 * falls back to the deterministic `tokenIcon(id)` choice when no override is
 * set. To clear an override, the user picks the "Default" item at the top.
 */

import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";
import {
  Activity,
  AlertTriangle,
  Archive,
  AtSign,
  BarChart,
  Bell,
  Bookmark,
  Bot,
  Box,
  Boxes,
  Braces,
  Calendar,
  Camera,
  CheckCircle,
  ClipboardList,
  Clock,
  Cloud,
  Code,
  Code2,
  Cog,
  Compass,
  Cpu,
  CreditCard,
  Database,
  DollarSign,
  Eye,
  FileCode,
  FileText,
  Filter,
  Fingerprint,
  Flag,
  Folder,
  FolderTree,
  Gauge,
  GitBranch,
  Globe,
  HardDrive,
  Hash,
  Headphones,
  Heart,
  Image,
  Inbox,
  Key,
  Layers,
  Layout,
  Link as LinkIcon,
  ListTree,
  Lock,
  Mail,
  Map,
  MapPin,
  MessageSquare,
  Mic,
  Monitor,
  Network,
  Package,
  Phone,
  PieChart,
  Plug,
  Puzzle,
  Receipt,
  Save,
  Search,
  Send,
  Server,
  Settings,
  Shield,
  ShoppingCart,
  Sliders,
  Smartphone,
  Sparkles,
  Star,
  Store,
  Tag,
  Table,
  Terminal,
  Timer,
  TrendingUp,
  Truck,
  User,
  Users,
  Video,
  Wifi,
  Workflow,
  Wrench,
  X,
  Zap,
} from "lucide-react";

export type IconName = keyof typeof ICONS;

export const ICONS: Record<string, ComponentType<LucideProps>> = {
  Activity, AlertTriangle, Archive, AtSign, BarChart, Bell, Bookmark, Bot,
  Box, Boxes, Braces, Calendar, Camera, CheckCircle, ClipboardList, Clock,
  Cloud, Code, Code2, Cog, Compass, Cpu, CreditCard, Database, DollarSign,
  Eye, FileCode, FileText, Filter, Fingerprint, Flag, Folder, FolderTree,
  Gauge, GitBranch, Globe, HardDrive, Hash, Headphones, Heart, Image, Inbox,
  Key, Layers, Layout, Link: LinkIcon, ListTree, Lock, Mail, Map, MapPin,
  MessageSquare, Mic, Monitor, Network, Package, Phone, PieChart, Plug,
  Puzzle, Receipt, Save, Search, Send, Server, Settings, Shield, ShoppingCart,
  Sliders, Smartphone, Sparkles, Star, Store, Tag, Table, Terminal, Timer,
  TrendingUp, Truck, User, Users, Video, Wifi, Workflow, Wrench, Zap,
};

export function lookupIcon(
  name: string | undefined,
): ComponentType<LucideProps> | null {
  if (!name) return null;
  return ICONS[name] ?? null;
}

export interface IconPickerProps {
  anchorRect: DOMRect;
  current: string | undefined;
  onPick: (name: string | undefined) => void;
  onClose: () => void;
}

const ICON_NAMES = Object.keys(ICONS);

export function IconPicker({
  anchorRect,
  current,
  onPick,
  onClose,
}: IconPickerProps) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement | null>(null);
  const containerRef = useRef<HTMLDivElement | null>(null);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return ICON_NAMES;
    return ICON_NAMES.filter((n) => n.toLowerCase().includes(q));
  }, [query]);

  useLayoutEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
      }
    };
    const onDown = (e: PointerEvent) => {
      const el = containerRef.current;
      if (el && !el.contains(e.target as Node)) onClose();
    };
    const t = setTimeout(() => window.addEventListener("pointerdown", onDown, true), 0);
    window.addEventListener("keydown", onKey);
    return () => {
      clearTimeout(t);
      window.removeEventListener("pointerdown", onDown, true);
      window.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const W = 260;
  const H = 320;
  const left = Math.min(anchorRect.left, window.innerWidth - W - 8);
  const top = Math.min(anchorRect.bottom + 4, window.innerHeight - H - 8);

  return createPortal(
    <div
      ref={containerRef}
      data-no-pickup
      onPointerDown={(e) => e.stopPropagation()}
      onWheel={(e) => e.stopPropagation()}
      style={{
        position: "fixed",
        left,
        top,
        width: W,
        zIndex: 1200,
      }}
      className="rounded border border-[var(--border-overlay)] bg-[var(--surface-overlay)] backdrop-blur-md shadow-xl"
    >
      <input
        ref={inputRef}
        type="text"
        placeholder="Search icons…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        className="w-full bg-transparent px-3 py-2 text-xs outline-none border-b border-[var(--border-subtle)] placeholder:text-[var(--text-ghost)]"
        style={{ color: "var(--text)" }}
      />
      <div className="grid grid-cols-8 gap-0.5 p-1.5 max-h-64 overflow-y-auto">
        <button
          type="button"
          onClick={() => {
            onPick(undefined);
            onClose();
          }}
          className={`flex items-center justify-center rounded p-1.5 hover:bg-[var(--surface-hover)] ${
            !current ? "bg-[var(--surface-hover)]" : ""
          }`}
          title="Default (auto-derived from id)"
        >
          <X className="h-4 w-4 text-[var(--text-ghost)]" />
        </button>
        {filtered.map((name) => {
          const Icon = ICONS[name];
          const active = current === name;
          return (
            <button
              key={name}
              type="button"
              onClick={() => {
                onPick(name);
                onClose();
              }}
              title={name}
              className={`flex items-center justify-center rounded p-1.5 hover:bg-[var(--surface-hover)] ${
                active ? "bg-[var(--surface-hover)]" : ""
              }`}
            >
              <Icon
                className={`h-4 w-4 ${active ? "text-[var(--text)]" : "text-[var(--text-secondary)]"}`}
              />
            </button>
          );
        })}
      </div>
    </div>,
    document.body,
  );
}
