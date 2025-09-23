// simple_smt.ts
import { createHash } from "crypto";

export type Bytes = Uint8Array;
type HashFn = (x: Bytes) => Bytes;

export const sha256: HashFn = (x) => createHash("sha256").update(Buffer.from(x)).digest();

// --- tiny byte helpers ---
const concat = (a: Bytes, b: Bytes): Bytes => {
    const o = new Uint8Array(a.length + b.length);
    o.set(a, 0); o.set(b, a.length);
    return o;
};
export const toHex = (b: Bytes) => "0x" + Array.from(b).map(x => x.toString(16).padStart(2, "0")).join("");
export const fromHex = (h: string) => {
    const s = h.startsWith("0x") ? h.slice(2) : h;
    const out = new Uint8Array(s.length / 2);
    for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(2 * i, 2 * i + 2), 16);
    return out;
};

const hashNode = (H: HashFn, L: Bytes, R: Bytes) => H(concat(new Uint8Array([0x01]), concat(L, R)));

export class SparseMerkleTree {
    readonly depth: number;      // number of edges from root to leaf (bottom level index width = 2^depth)
    readonly H: HashFn;
    readonly capacity: number;   // 2^depth (bounded for practicality)
    private defaults: Bytes[];   // default hash at each level: defaults[0] = empty leaf, ..., defaults[depth] = empty root

    readonly EMPTY: Bytes;

    /** Leaf storage: an array of *leaf hashes*; undefined means "empty/default" */
    private leaves: Bytes[];
    /** Append pointer: first index we’ll try for the next insert */
    private nextFree = 0;

    constructor(opts?: { depth?: number; hash?: HashFn }) {
        this.depth = opts?.depth ?? 16;       // default small; set what you need
        if (this.depth < 0) throw new Error("depth must be >= 0");
        // Practical bound: array size is 2^depth; guard to avoid accidental huge allocs
        if (this.depth > 22) {
            // 2^22 = ~4.19M leaves; adjust if you really want larger
            throw new Error("depth too large for an array-backed bottom tree (max 22 by default)");
        }

        this.H = opts?.hash ?? sha256;
        this.capacity = 1 << this.depth;

        this.defaults = this.precomputeDefaults();
        this.EMPTY = this.defaults[0];

        this.leaves = new Array(this.capacity); // all undefined by default (treated as empty)
    }

    /* ---------- Public API ---------- */

    getNextFree(): number {
        return this.nextFree
    }

    /** Append a leaf at the next empty slot. Returns the index used. */
    insertLeaf(value: Bytes): number {
        if (toHex(value) === toHex(this.EMPTY)) {
            throw new Error("insertLeaf: cannot insert EMPTY as a leaf");
        }
        let i = this.nextFree;
        while (i < this.capacity && toHex(this.leaves[i]) !== toHex(this.EMPTY)) i++;
        if (i >= this.capacity) throw new Error("insertLeaf: tree is full");

        this.leaves[i] = value;
        this.nextFree = i + 1;
        return i;
    }

    /** Set a leaf at specific index (expects already a hash). */
    setLeaf(index: number, hash: Bytes): void {
        this.assertIndex(index);
        this.leaves[index] = hash;
        if (index >= this.nextFree) this.nextFree = index + 1;
    }

    /** Return current root. */
    getRoot(): Bytes {
        let levelNodes: Bytes[] = [...this.leaves];

        for (let level = 1; level <= this.depth; level++) {
            const parentCount = levelNodes.length >> 1;
            const next: Bytes[] = new Array(parentCount);
            for (let i = 0; i < parentCount; i++) {
                const L = levelNodes[i * 2] ?? this.defaults[level - 1];
                const R = levelNodes[i * 2 + 1] ?? this.defaults[level - 1];
                next[i] = hashNode(this.H, L, R);
            }
            levelNodes = next;
        }
        return levelNodes[0] ?? this.defaults[this.depth];
    }

    /** Return all leaf hashes (default/empty included as undefined). */
    getLeaves(): Bytes[] {
        // shallow copy so callers can't mutate internal state directly
        return [...this.leaves];
    }

    /**
     * Get inclusion proof for leaf at `index`.
     * Returns an array of sibling hashes from bottom (level 0) up to level depth-1.
     */
    getProof(index: number): Bytes[] {
        this.assertIndex(index);

        // Materialize the bottom level (leaf hashes/defaults)
        let levelNodes: Bytes[] = new Array(this.capacity);
        for (let i = 0; i < this.capacity; i++) {
            levelNodes[i] = this.leaves[i] ?? this.defaults[0];
        }

        const proof: Bytes[] = [];
        let idx = index;

        // For each level, sibling is idx^1 before collapsing upward
        for (let level = 0; level < this.depth; level++) {
            const siblingIdx = idx ^ 1;
            const sibling = levelNodes[siblingIdx] ?? this.defaults[level];
            proof.push(sibling);

            // build next level for *all* nodes, but we can do it lazily for performance.
            const parentCount = levelNodes.length >> 1;
            const next: Bytes[] = new Array(parentCount);
            for (let i = 0; i < parentCount; i++) {
                const L = levelNodes[i * 2];
                const R = levelNodes[i * 2 + 1] ?? this.defaults[level];
                next[i] = hashNode(this.H, L, R);
            }
            levelNodes = next;
            idx = idx >> 1; // parent index
        }

        return proof;
    }

    /** Verify proof given leaf (already a hash). */
    verify(index: number, leaf: Bytes, proof: Bytes[], root: Bytes): boolean {
        if (proof.length !== this.depth) return false;
        this.assertIndex(index);

        let acc = leaf;
        let idx = index;

        for (let level = 0; level < this.depth; level++) {
            const sib = proof[level];
            const left = (idx & 1) === 0 ? acc : sib;
            const right = (idx & 1) === 0 ? sib : acc;
            acc = hashNode(this.H, left, right);
            idx >>= 1;
        }
        return toHex(acc) === toHex(root);
    }

    /** Returns true if any leaf equals `hash` (and isn’t EMPTY). */
    hasLeaf(hash: Bytes): { found: boolean, index: number } {
        if (SparseMerkleTree.eq(hash, this.EMPTY)) return {
            found: false,
            index: 0,
        };
        for (let i = 0; i < this.capacity; i++) {
            const h = this.leaves[i];
            if (SparseMerkleTree.eq(h, hash)) return {
                found: true,
                index: i,
            };
        }
        return {
            found: false,
            index: 0,
        };
    }

    deleteLeaf(index: number) {
        // Remove that index and shift everything left
        this.leaves.splice(index, 1);

        // Push EMPTY at the end to maintain fixed capacity
        this.leaves.push(this.EMPTY);

        // Update nextFree: the first EMPTY index from the left
        this.nextFree = this.leaves.findIndex(
            (h) => SparseMerkleTree.eq(h, this.EMPTY)
        );
    }


    /* ---------- Internals ---------- */

    private static eq(a: Bytes, b: Bytes): boolean {
        if (a.length !== b.length) return false;
        let acc = 0;
        for (let i = 0; i < a.length; i++) acc |= a[i] ^ b[i];
        return acc === 0;
    }

    private assertIndex(index: number) {
        if (!Number.isInteger(index) || index < 0 || index >= this.capacity) {
            throw new Error(`index out of range (0..${this.capacity - 1})`);
        }
    }

    private precomputeDefaults(): Bytes[] {
        const defs: Bytes[] = new Array(this.depth + 1);
        // define EMPTY as hash of 32 zero bytes for consistency
        const zero = new Uint8Array(32);
        defs[0] = zero;
        for (let lvl = 1; lvl <= this.depth; lvl++) {
            defs[lvl] = hashNode(this.H, defs[lvl - 1], defs[lvl - 1]);
        }
        return defs;
    }
}