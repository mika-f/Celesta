// A minimal react-reconciler host config. The "DOM" here is a plain tree of
// {type, props, children} instances — there is no real host platform, just
// data collected for render.ts to walk into a Scene. Mutation mode is used
// because it is the simplest mode to implement correctly; this renderer is
// not performance-sensitive (one composition, re-rendered once per exported
// frame, not sixty times a second against a live display).

import * as React from 'react';
import Reconciler from 'react-reconciler';
import { ConcurrentRoot, DefaultEventPriority, NoEventPriority } from 'react-reconciler/constants';

export interface HostNode {
  type: string;
  props: Record<string, unknown>;
  children: HostNode[];
  /** `null` for a child of the container or a node that is not attached. */
  parent: HostNode | null;
  /** The `hostRevision()` of the last change to this node or below it. */
  changedAt: number;
  /** The React fiber that created or last updated this node, for `sourceChain`. */
  fiber?: unknown;
}

// Counts changes React makes to any host tree. Each change stamps the node it
// touches and that node's ancestors, so render.ts can tell whether anything
// below a `<Static>` changed since it last walked it.
let revision = 0;

/** The number of the last change React made to a host tree. */
export function hostRevision(): number {
  return revision;
}

function touch(node: HostNode | null): void {
  revision += 1;
  for (let current = node; current && current.changedAt !== revision; current = current.parent) {
    current.changedAt = revision;
  }
}

/**
 * Whether two props objects hold the same values, by `Object.is`. A host
 * node's element children are its child nodes, not props, so `children`
 * only counts for text, which keeps its content in props.
 */
function sameProps(type: string, a: Record<string, unknown>, b: Record<string, unknown>): boolean {
  const keys = Object.keys(a);
  if (keys.length !== Object.keys(b).length) return false;
  return keys.every((key) =>
    (key === 'children' && type !== 'text') || (Object.prototype.hasOwnProperty.call(b, key) && Object.is(a[key], b[key])));
}

function attach(parent: HostNode | null, child: HostNode): void {
  child.parent = parent;
  touch(parent);
}

export interface RootContainer {
  children: HostNode[];
}

type NoTimeout = -1;
const NO_TIMEOUT: NoTimeout = -1;
const NO_CONTEXT = {};

function removeChild(children: HostNode[], child: HostNode): void {
  const index = children.indexOf(child);
  if (index !== -1) {
    children.splice(index, 1);
  }
}

function appendChild(children: HostNode[], child: HostNode): void {
  removeChild(children, child);
  children.push(child);
}

function insertBefore(children: HostNode[], child: HostNode, beforeChild: HostNode): void {
  removeChild(children, child);
  const index = children.indexOf(beforeChild);
  children.splice(index === -1 ? children.length : index, 0, child);
}

// The priority of the update React is scheduling, which it sets around the
// updates it makes itself. Without events of its own, this renderer has no
// other source for it.
let currentUpdatePriority: number = NoEventPriority;

const hostConfig: Reconciler.HostConfig<
  string,
  Record<string, unknown>,
  RootContainer,
  HostNode,
  never,
  never,
  never,
  never,
  never,
  HostNode,
  typeof NO_CONTEXT,
  never,
  ReturnType<typeof setTimeout>,
  NoTimeout,
  null,
  null,
  null,
  never,
  never,
  never
> = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,
  isPrimaryRenderer: true,

  rendererPackageName: '@celesta.studio/react',
  rendererVersion: React.version,
  extraDevToolsConfig: null,
  bindToConsole: (methodName, args) => console[methodName as 'error'].bind(console, ...args),

  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: NO_TIMEOUT,

  getRootHostContext: () => NO_CONTEXT,
  getChildHostContext: (parentHostContext) => parentHostContext,
  prepareForCommit: () => null,
  resetAfterCommit: () => {},

  // Text content stays on the host props instead of becoming a separate
  // instance, so render.ts can read it back with `extractText`.
  shouldSetTextContent: (type) => type === 'text',
  createTextInstance: () => {
    throw new Error(
      'bare text is only supported inside <Text>; other elements only accept element children',
    );
  },

  createInstance(type, props, _root, _hostContext, internalInstanceHandle) {
    return { type, props, children: [], parent: null, changedAt: 0, fiber: internalInstanceHandle };
  },
  appendInitialChild(parent, child) {
    parent.children.push(child);
    attach(parent, child);
  },
  finalizeInitialChildren: () => false,

  // React also calls these to move a child that is already mounted (a keyed
  // list reordered between frames) and expects DOM semantics: the child
  // leaves its old position. Inserting it without removing it first leaves a
  // duplicate behind on every move, so a list re-sorted each frame grows
  // without bound and its layers are drawn several times over.
  appendChildToContainer(container, child) {
    appendChild(container.children, child);
    attach(null, child);
  },
  appendChild(parent, child) {
    appendChild(parent.children, child);
    attach(parent, child);
  },
  insertBefore(parent, child, beforeChild) {
    insertBefore(parent.children, child, beforeChild);
    attach(parent, child);
  },
  insertInContainerBefore(container, child, beforeChild) {
    insertBefore(container.children, child, beforeChild);
    attach(null, child);
  },
  removeChild(parent, child) {
    removeChild(parent.children, child);
    child.parent = null;
    touch(parent);
  },
  removeChildFromContainer(container, child) {
    removeChild(container.children, child);
    child.parent = null;
    touch(null);
  },
  clearContainer(container) {
    container.children = [];
  },

  // React renders a component again whenever a context it reads changes,
  // and the frame's runtime context changes every frame; most of the host
  // props it then produces are the same. Leaving those uncommitted keeps a
  // `<Static>` with groups inside from looking changed on every frame.
  commitUpdate(instance, type, oldProps, newProps, internalInstanceHandle) {
    if (sameProps(type, oldProps, newProps)) return;
    instance.props = newProps;
    instance.fiber = internalInstanceHandle;
    touch(instance);
  },

  getPublicInstance: (instance) => instance,
  preparePortalMount: () => {},
  detachDeletedInstance: () => {},

  setCurrentUpdatePriority: (priority) => {
    currentUpdatePriority = priority;
  },
  getCurrentUpdatePriority: () => currentUpdatePriority,
  resolveUpdatePriority: () =>
    currentUpdatePriority === NoEventPriority ? DefaultEventPriority : currentUpdatePriority,

  // Nothing here suspends a commit: assets load before the frame renders
  // (render.ts awaits them), not while React commits it.
  maySuspendCommit: () => false,
  maySuspendCommitOnUpdate: () => false,
  maySuspendCommitInSyncRender: () => false,
  preloadInstance: () => true,
  startSuspendingCommit: () => null,
  suspendInstance: () => {},
  suspendOnActiveViewTransition: () => {},
  waitForCommitToBeReady: () => null,
  getSuspendedCommitReason: () => null,

  // Event, form, transition, and DevTools integration this renderer has no
  // use for (there is no real host platform to dispatch DOM-style events
  // against), but the HostConfig type marks them required.
  NotPendingTransition: null,
  HostTransitionContext: React.createContext(null) as unknown as Reconciler.ReactContext<null>,
  resetFormInstance: () => {},
  requestPostPaintCallback: () => {},
  shouldAttemptEagerTransition: () => false,
  trackSchedulerEvent: () => {},
  resolveEventType: () => null,
  resolveEventTimeStamp: () => -1.1,
  getInstanceFromNode: () => null,
  beforeActiveInstanceBlur: () => {},
  afterActiveInstanceBlur: () => {},
  prepareScopeUpdate: () => {},
  getInstanceFromScope: () => null,
};

const HostReconciler = Reconciler(hostConfig);

export interface HostRoot {
  container: RootContainer;
  /**
   * Renders `element` into the container and commits it before returning,
   * effects included. An error no error boundary caught is thrown from here.
   */
  render(element: React.ReactNode): void;
}

export function createRoot(): HostRoot {
  const container: RootContainer = { children: [] };
  // React reports an error no boundary caught to `onUncaughtError` during the
  // commit and defers anything that callback throws to a timer, which would
  // crash the process instead of failing the frame. Keep it to throw from
  // `render` once the commit is done.
  const uncaught: unknown[] = [];
  const root = HostReconciler.createContainer(
    container,
    ConcurrentRoot,
    null,
    false,
    null,
    '',
    (error) => {
      uncaught.push(error);
    },
    (error) => {
      console.error(error);
    },
    (error) => {
      console.error(error);
    },
    () => {},
    null,
  );
  return {
    container,
    render(element) {
      uncaught.length = 0;
      HostReconciler.updateContainerSync(element, root, null, null);
      HostReconciler.flushSyncWork();
      if (uncaught.length > 0) {
        const [error] = uncaught;
        uncaught.length = 0;
        throw error instanceof Error ? error : new Error(String(error));
      }
    },
  };
}
