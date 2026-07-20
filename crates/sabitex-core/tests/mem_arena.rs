//! Invariant tests for the `mem` allocator (tex.web Part 9, §163-§164).
//! Checks mirror the `check_mem` debugging procedure of §167.

use sabitex_core::mem::{Mem, EMPTY_FLAG, HI_MEM_STAT_USAGE, LO_MEM_STAT_SPAN};
use sabitex_core::types::{Pointer, MAX_HALFWORD, NULL};

const MEM_TOP: Pointer = 2000; // the small TRIP-style setting

fn fresh() -> Mem {
    Mem::new(MEM_TOP, 0)
}

/// Walks the ring of empties and returns (locations, total free words),
/// verifying the doubly-linked invariants of §124.
fn audit_ring(mem: &Mem) -> (Vec<Pointer>, i32) {
    let mut locs = Vec::new();
    let mut free = 0;
    let mut p = mem.rover;
    loop {
        assert!(mem.is_empty(p), "ring node {p} must be empty");
        assert!(mem.node_size(p) >= 2, "empty nodes have size >= 2 (§124)");
        assert!(p + mem.node_size(p) <= mem.lo_mem_max);
        assert_eq!(mem.llink(mem.rlink(p)), p, "llink(rlink(p)) = p at {p}");
        locs.push(p);
        free += mem.node_size(p);
        p = mem.rlink(p);
        if p == mem.rover {
            break;
        }
    }
    (locs, free)
}

#[test]
fn initial_layout_matches_section_164() {
    let mem = fresh();
    assert_eq!(mem.rover, mem.lo_mem_stat_max() + 1);
    assert_eq!(mem.node_size(mem.rover), 1000);
    assert_eq!(mem.link(mem.rover), EMPTY_FLAG);
    assert_eq!(mem.llink(mem.rover), mem.rover);
    assert_eq!(mem.rlink(mem.rover), mem.rover);
    assert_eq!(mem.lo_mem_max, mem.rover + 1000);
    assert_eq!(mem.avail, NULL);
    assert_eq!(mem.mem_end, MEM_TOP);
    assert_eq!(mem.hi_mem_min, MEM_TOP - 13);
    assert_eq!(mem.var_used, LO_MEM_STAT_SPAN + 1);
    assert_eq!(mem.dyn_used, HI_MEM_STAT_USAGE);
    // The permanent list heads are cleared, except for the special heads
    // initialized by §819 (active), §981 (page_ins_head), §988 (page_head),
    // and §797 (end_span).
    let special = [MEM_TOP, MEM_TOP - 2, MEM_TOP - 6, MEM_TOP - 7, MEM_TOP - 9];
    for k in mem.hi_mem_min..=MEM_TOP {
        if special.contains(&k) {
            continue;
        }
        assert_eq!(mem.link(k), NULL);
        assert_eq!(mem.info(k), NULL);
    }
    // §819: the active-list sentinel ends every active list.
    assert_eq!(
        mem.node_type(MEM_TOP - 7),
        sabitex_core::linebreak::HYPHENATED
    );
    assert_eq!(mem.llink(MEM_TOP - 7), MAX_HALFWORD); // line_number
                                                      // §981: page_ins_head is its own successor with the largest subtype.
    assert_eq!(mem.link(MEM_TOP), MEM_TOP);
    assert_eq!(mem.subtype(MEM_TOP), 255);
    // §988: the current page conceptually starts with glue.
    assert_eq!(mem.node_type(MEM_TOP - 2), sabitex_core::nodes::GLUE_NODE);
    assert_eq!(mem.link(MEM_TOP - 2), NULL);
    // §797: end_span has the largest possible span count.
    assert_eq!(mem.link(MEM_TOP - 9), 0x10000);
    assert_eq!(mem.info(MEM_TOP - 9), NULL);
}

#[test]
fn get_node_allocates_from_the_top_and_frees_back() {
    let mut mem = fresh();
    let v0 = mem.var_used;

    // §128: allocation comes from the top of the rover node.
    let p = mem.get_node(10).unwrap();
    assert_eq!(p, mem.rover + 990);
    assert_eq!(mem.link(p), NULL);
    assert_eq!(mem.node_size(mem.rover), 990);
    assert_eq!(mem.var_used, v0 + 10);

    let q = mem.get_node(7).unwrap();
    assert_eq!(q, mem.rover + 983);
    assert_eq!(mem.var_used, v0 + 17);

    // Free in allocation order; ring stays consistent and sizes add up.
    mem.free_node(p, 10);
    mem.free_node(q, 7);
    assert_eq!(mem.var_used, v0);
    let (_, free) = audit_ring(&mem);
    assert_eq!(free, 983 + 10 + 7);

    // After freeing everything, a full-size request succeeds again
    // (the merge pass of §127 coalesces the fragments).
    let r = mem.get_node(1000).unwrap();
    assert_eq!(mem.var_used, v0 + 1000);
    mem.free_node(r, 1000);
}

#[test]
fn get_node_grows_the_lower_region_when_needed() {
    let mut mem = fresh();
    let lo_before = mem.lo_mem_max;

    // Exhaust the initial 1000-word node, then ask for more: §126 grows
    // lo_mem_max (by half the remaining gap here, since
    // hi_mem_min - lo_mem_max < 1998 with the small TRIP-style mem_top).
    let a = mem.get_node(998).unwrap();
    let b = mem.get_node(500).unwrap();
    assert!(b > lo_before, "second node must come from grown memory");
    assert!(mem.lo_mem_max > lo_before);
    assert!(mem.lo_mem_max < mem.hi_mem_min);
    mem.free_node(a, 998);
    mem.free_node(b, 500);
}

#[test]
fn get_node_overflows_when_memory_is_exhausted() {
    let mut mem = fresh();
    // Keep allocating mid-size nodes until the allocator must give up:
    // growth stops when lo_mem_max + 2 reaches hi_mem_min (§125).
    let mut count = 0;
    loop {
        match mem.get_node(100) {
            Ok(_) => count += 1,
            Err(e) => {
                assert!(e.to_string().contains("main memory size"), "{e}");
                break;
            }
        }
        assert!(count < 1000, "must overflow eventually");
    }
    // Roughly (mem_top - statics) / 100 nodes fit.
    assert!(count >= 15, "got {count}");
}

#[test]
fn merging_coalesces_adjacent_free_neighbors() {
    let mut mem = fresh();
    let p1 = mem.get_node(100).unwrap();
    let p2 = mem.get_node(100).unwrap();
    let p3 = mem.get_node(100).unwrap();
    // Free two physically adjacent blocks (p2 and p1 are neighbors at the
    // top of the region), allocate again, and check that the ring stays
    // consistent and accounts for every freed word.
    mem.free_node(p2, 100);
    mem.free_node(p1, 100);
    let big = mem.get_node(150).unwrap();
    audit_ring(&mem);
    mem.free_node(big, 150);
    mem.free_node(p3, 100);
    let (_, free) = audit_ring(&mem);
    assert_eq!(free, 1000);
}

#[test]
fn one_word_allocation_via_avail_stack() {
    let mut mem = fresh();
    let d0 = mem.dyn_used;
    let hi0 = mem.hi_mem_min;

    // mem_end == mem_max here, so virgin territory is exhausted: get_avail
    // decrements hi_mem_min (§120).
    let p = mem.get_avail().unwrap();
    assert_eq!(p, hi0 - 1);
    assert_eq!(mem.link(p), NULL);
    assert_eq!(mem.dyn_used, d0 + 1);

    // free_avail pushes onto the avail stack; the next get_avail pops it.
    mem.free_avail(p);
    assert_eq!(mem.dyn_used, d0);
    assert_eq!(mem.avail, p);
    let q = mem.get_avail().unwrap();
    assert_eq!(q, p);

    // flush_list returns a whole chain.
    let a = mem.get_avail().unwrap();
    let b = mem.get_avail().unwrap();
    let c = mem.get_avail().unwrap();
    mem.set_link(a, b);
    mem.set_link(b, c);
    mem.set_link(c, NULL);
    let d_before = mem.dyn_used;
    mem.flush_list(a);
    assert_eq!(mem.dyn_used, d_before - 3);
    assert_eq!(mem.avail, a);

    // Exhaustion: one-word region collides with lo_mem_max.
    let mut last = Ok(0);
    for _ in 0..(MEM_TOP as usize) {
        last = mem.get_avail();
        if last.is_err() {
            break;
        }
    }
    assert!(last.is_err(), "get_avail must overflow eventually");
}

#[test]
fn sort_avail_orders_the_ring_by_location() {
    let mut mem = fresh();
    let p1 = mem.get_node(50).unwrap();
    let p2 = mem.get_node(60).unwrap();
    let p3 = mem.get_node(70).unwrap();
    let p4 = mem.get_node(80).unwrap();
    // Free in scrambled order with live nodes between, so several
    // non-adjacent empties exist.
    mem.free_node(p3, 70);
    mem.free_node(p1, 50);
    mem.sort_avail().unwrap();
    let (locs, _) = audit_ring(&mem);
    let mut sorted = locs.clone();
    sorted.sort_unstable();
    assert_eq!(locs, sorted, "§131: smallest location first");
    assert_eq!(locs[0], mem.rover);
    assert!(locs.windows(2).all(|w| w[0] < w[1]));
    // rlink of the largest wraps to rover; checked by audit_ring's
    // llink/rlink consistency pass. EMPTY_FLAG never leaks as a link.
    assert!(locs.iter().all(|&p| mem.rlink(p) != MAX_HALFWORD));
    mem.free_node(p2, 60);
    mem.free_node(p4, 80);
}
