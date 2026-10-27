// Independent differential driver for the Festival 3.1 C++ reference.
// Copyright 2018-2022 Yaron Shoham (solver); driver: festival-rs contributors.
//
// Build with scripts/build_refcheck.sh. This driver does not change solver
// behavior; it only exposes board_is_deadlocked / move_is_deadlocked for
// differential testing against festival-rs.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <chrono>
#include <vector>
#include <unordered_set>

#include "global.h"
#include "board.h"
#include "moves.h"
#include "helper.h"
#include "perimeter.h"
#include "deadlock_cache.h"
#include "advanced_deadlock.h"
#include "deadlock_utils.h"
#include "preprocess.h"
#include "io.h"
#include "packing_search.h"
#include "tree.h"
#include "util.h"
#include "park_order.h"
#include "score.h"
#include "girl.h"
#include "dragonfly.h"

extern dragonfly_node* dragonfly_nodes;
extern int dragonfly_nodes_num;
extern int dragonfly_roots_num;
extern int dragonfly_max_nodes;
extern int total_perimeter_entries;
int perimeter_is_full();
void dragonfly_get_board(dragonfly_node* e, board b);

void allocate_deadlock_cache();
void read_deadlock_patterns(int pull_mode);
extern tree search_trees[8];
extern helper helpers[8];
void allocate_search_trees();
void free_search_trees();
void allocate_helpers();
void free_helpers();
int preprocess_level(board b);
int board_is_deadlocked(board b, int pull_mode, int search_mode);
int advanced_deadlock_pull_mode(board b_in, int without_wobblers);
int advanced_deadlock_search(board b, int pull_mode);
int work_left_outside_deadlock_zone(board b);
void set_deadlock_zone_in_pull_mode(board b);
void clear_boxes_out_of_deadlock_zone(board b);
void clear_bases_inplace(board b);
int boxes_in_level(board b);
int get_connectivity(board b);
int check_matching_deadlock(board b, int pull_mode, int search_mode);
int is_stuck_deadlock(board b, int pull_mode);
int is_diag_deadlock(board b);
int is_k_dist_deadlock(board b);
int is_wobblers_deadlock(board b, int pull_mode);
int try_pulling_all_zones(board b);
int corral_deadlock(board b, int pull_mode);

static unsigned long long rng_state;

typedef struct board_copy
{
	unsigned char cells[MAX_SIZE][MAX_SIZE];
} board_copy;

static void copy_to_pool(board b, board_copy* out)
{
	memcpy(out->cells, b, sizeof(board));
}

static void copy_from_pool(const board_copy* in, board b)
{
	memcpy(b, in->cells, sizeof(board));
}

static unsigned int next_rand()
{
	rng_state ^= rng_state << 13;
	rng_state ^= rng_state >> 7;
	rng_state ^= rng_state << 17;
	return (unsigned int)(rng_state >> 16);
}

static void board_to_hex(board b, FILE* fp)
{
	int y, x;
	for (y = 0; y < height; y++)
		for (x = 0; x < width; x++)
			fprintf(fp, "%02x", b[y][x]);
}

static void raw_moves(board b, std::vector<move>& pushes, std::vector<move>& pulls)
{
	int i, d;
	int y, x;

	pushes.clear();
	pulls.clear();

	for (i = 0; i < index_num; i++)
	{
		index_to_y_x(i, &y, &x);
		if ((b[y][x] & BOX) == 0) continue;

		for (d = 0; d < 4; d++)
		{
			int dy = delta_y[d];
			int dx = delta_x[d];

			int to_y = y + dy;
			int to_x = x + dx;
			int pusher_y = y - dy;
			int pusher_x = x - dx;

			if (to_y < 0 || to_x < 0 || to_y >= height || to_x >= width) continue;

			if ((b[to_y][to_x] & OCCUPIED) == 0)
			{
				int to_index = y_x_to_index(to_y, to_x);
				if (to_index >= 0 && b[pusher_y][pusher_x] & SOKOBAN)
				{
					move m;
					memset(&m, 0, sizeof(m));
					m.from = i;
					m.to = to_index;
					m.sokoban_position = (d + 2) & 3;
					pushes.push_back(m);
				}
			}

			int pull_to_y = y + dy * 2;
			int pull_to_x = x + dx * 2;
			if (pull_to_y < 0 || pull_to_x < 0 || pull_to_y >= height || pull_to_x >= width) continue;
			if ((b[to_y][to_x] & SOKOBAN) == 0) continue;
			if (b[pull_to_y][pull_to_x] & OCCUPIED) continue;

			int pull_to_index = y_x_to_index(pull_to_y, pull_to_x);
			if (pull_to_index < 0) continue;

			move p;
			memset(&p, 0, sizeof(p));
			p.from = i;
			p.to = pull_to_index;
			p.sokoban_position = (d + 2) & 3;
			p.pull = 1;
			pulls.push_back(p);
		}
	}
}

static void add_successor(std::vector<board_copy>& pool, std::unordered_set<unsigned long long>& seen, board b)
{
	UINT_64 hash;
	board_copy copy;
	if (pool.size() >= 200000) return;
	hash = get_board_hash(b);
	if (seen.count(hash)) return;
	seen.insert(hash);
	copy_to_pool(b, &copy);
	pool.push_back(copy);
}

static int run_deadlock(int argc, char** argv)
{
	const char* levels = argv[2];
	int level_number = atoi(argv[3]);
	int target = atoi(argv[4]);
	int move_samples = atoi(argv[5]);
	int seed = atoi(argv[6]);
	const char* prefix = argv[7];

	board start;
	board b;
	int i;
	int produced = 0;
	int attempts = 0;
	int move_saved = 0;
	long long board_calls = 0;
	long long move_calls = 0;
	long long board_ns = 0;
	long long move_ns = 0;
	std::vector<board_copy> pool;
	std::unordered_set<unsigned long long> seen;
	helper h;
	char boards_path[2048];
	char moves_path[2048];
	FILE *fb, *fm;

	verbose = 0;
	cores_num = 1;
	extra_mem = 0;
	time_limit = 600;
	start_time = (int)time(0);
	rng_state = (unsigned long long)(seed ? seed : 1);

	strcpy(global_level_set_name, levels);
	load_level_from_file(start, level_number);
	if (preprocess_level(start) == 0)
	{
		fprintf(stderr, "level %d failed preprocessing\n", level_number);
		return 1;
	}

	snprintf(boards_path, sizeof(boards_path), "%s.boards", prefix);
	snprintf(moves_path, sizeof(moves_path), "%s.moves", prefix);
	fb = fopen(boards_path, "wb");
	fm = fopen(moves_path, "wb");
	if (fb == NULL || fm == NULL)
	{
		fprintf(stderr, "can't open output files\n");
		return 1;
	}

	init_helper(&h);

	copy_board(start, b);
	seen.insert(get_board_hash(b));
	{
		board_copy copy;
		copy_to_pool(b, &copy);
		pool.push_back(copy);
	}

	while (produced < target && !pool.empty() && attempts < target * 100)
	{
		size_t index = next_rand() % pool.size();
		std::vector<move> pushes;
		std::vector<move> pulls;
		board current;
		copy_from_pool(&pool[index], current);
		UINT_64 hash = get_board_hash(current);
		(void)hash;

		pool[index] = pool.back();
		pool.pop_back();
		attempts++;

		fprintf(fb, "%llu", (unsigned long long)get_board_hash(current));
		fputc(' ', fb);
		board_to_hex(current, fb);
		{
			int push_deadlock;
			int pull_deadlock;
			std::chrono::steady_clock::time_point t0 = std::chrono::steady_clock::now();
			push_deadlock = board_is_deadlocked(current, 0, NORMAL);
			pull_deadlock = board_is_deadlocked(current, 1, NORMAL);
			std::chrono::steady_clock::time_point t1 = std::chrono::steady_clock::now();
			board_ns += std::chrono::duration_cast<std::chrono::nanoseconds>(t1 - t0).count();
			board_calls += 2;
			fprintf(fb, " %d %d\n", push_deadlock, pull_deadlock);
		}
		produced++;

		raw_moves(current, pushes, pulls);

		if (move_saved < move_samples)
		{
			size_t k;
			std::vector<int> push_verdicts;
			std::vector<int> pull_verdicts;
			std::chrono::steady_clock::time_point t0 = std::chrono::steady_clock::now();
			for (k = 0; k < pushes.size(); k++)
				push_verdicts.push_back(move_is_deadlocked(current, &pushes[k], 0, NORMAL));
			for (k = 0; k < pulls.size(); k++)
				pull_verdicts.push_back(move_is_deadlocked(current, &pulls[k], 1, NORMAL));
			std::chrono::steady_clock::time_point t1 = std::chrono::steady_clock::now();
			move_ns += std::chrono::duration_cast<std::chrono::nanoseconds>(t1 - t0).count();
			move_calls += (long long)(pushes.size() + pulls.size());

			fprintf(fm, "%d %d", produced - 1, (int)pushes.size());
			for (k = 0; k < push_verdicts.size(); k++)
				fprintf(fm, " %d", push_verdicts[k]);
			fprintf(fm, " %d", (int)pulls.size());
			for (k = 0; k < pull_verdicts.size(); k++)
				fprintf(fm, " %d", pull_verdicts[k]);
			fputc('\n', fm);
			move_saved++;
		}

		/* Deterministic walk using the solver's pruned macro moves. */
		{
			move moves[MAX_MOVES];
			int corral = 0;
			int n = find_possible_moves(current, moves, 0, &corral, NORMAL, &h);
			for (i = 0; i < n; i++)
			{
				if (moves[i].kill) continue;
				board c;
				copy_board(current, c);
				apply_move(c, &moves[i], NORMAL);
				add_successor(pool, seen, c);
			}
		}

		/* Also include every physically legal single push/pull successor so
		   that deadlocked positions are represented. */
		{
			size_t k;
			for (k = 0; k < pushes.size(); k++)
			{
				board c;
				copy_board(current, c);
				apply_move(c, &pushes[k], NORMAL);
				add_successor(pool, seen, c);
			}
			for (k = 0; k < pulls.size(); k++)
			{
				board c;
				copy_board(current, c);
				apply_move(c, &pulls[k], NORMAL);
				add_successor(pool, seen, c);
			}
		}
	}

	fclose(fb);
	fclose(fm);

	{
		char info_path[2048];
		FILE* fi;
		snprintf(info_path, sizeof(info_path), "%s.info", prefix);
		fi = fopen(info_path, "wb");
		if (fi != NULL)
		{
			fprintf(fi, "produced %d\n", produced);
			fprintf(fi, "board_calls %lld\n", board_calls);
			fprintf(fi, "board_ns %lld\n", board_ns);
			fprintf(fi, "move_calls %lld\n", move_calls);
			fprintf(fi, "move_ns %lld\n", move_ns);
			fclose(fi);
		}
	}

	printf("level %d: produced %d boards, %d with move checks, pool %d\n",
		level_number, produced, move_saved, (int)pool.size());
	return 0;
}

static int run_explain(int argc, char** argv)
{
	const char* levels = argv[2];
	int level_number = atoi(argv[3]);
	const char* hex = argv[4];
	board start, b, c;
	int y, x, res;

	verbose = 0;
	cores_num = 1;
	extra_mem = 0;
	time_limit = 600;
	start_time = (int)time(0);
	strcpy(global_level_set_name, levels);
	load_level_from_file(start, level_number);
	if (preprocess_level(start) == 0)
	{
		fprintf(stderr, "level %d failed preprocessing\n", level_number);
		return 1;
	}

	memset(b, 0, sizeof(board));
	for (y = 0; y < height; y++)
		for (x = 0; x < width; x++)
		{
			unsigned int value;
			sscanf(hex + 2 * (y * width + x), "%02x", &value);
			b[y][x] = (unsigned char)value;
		}

	print_board(b);
	printf("boxes %d / %d, connectivity %d\n", boxes_in_level(b), global_boxes_in_level,
		get_connectivity(b));

	if (argc >= 6)
	{
		const char* which = argv[5];
		int res = -1;
		if (strcmp(which, "matching") == 0)
			res = check_matching_deadlock(b, 0, NORMAL);
		else if (strcmp(which, "pullzones") == 0)
			res = try_pulling_all_zones(b);
		else if (strcmp(which, "stuck") == 0)
			res = is_stuck_deadlock(b, 0);
		else if (strcmp(which, "corral") == 0)
			res = corral_deadlock(b, 0);
		else if (strcmp(which, "diag") == 0)
			res = is_diag_deadlock(b);
		else if (strcmp(which, "kdist") == 0)
			res = is_k_dist_deadlock(b);
		else if (strcmp(which, "wobblers") == 0)
			res = is_wobblers_deadlock(b, 0);
		printf("%s -> %d\n", which, res);
		return 0;
	}

	copy_board(b, c);
	clear_bases_inplace(c);
	set_deadlock_zone_in_pull_mode(c);
	printf("pull zone size %d\n", size_of_deadlock_zone(c));
	for (y = 0; y < height; y++)
	{
		for (x = 0; x < width; x++)
			printf("%02x ", c[y][x]);
		printf("\n");
	}
	print_board(c);
	clear_boxes_out_of_deadlock_zone(c);
	printf("cleared boxes %d / %d\n", boxes_in_level(c), global_boxes_in_level);
	print_board(c);
	res = advanced_deadlock_search(c, 1);
	printf("advanced_deadlock_search -> %d\n", res);
	printf("work_left_outside %d\n", work_left_outside_deadlock_zone(b));
	printf("pull_mode_deadlock %d\n", advanced_deadlock_pull_mode(b, 0));
	printf("board_is_deadlocked(1) %d\n", board_is_deadlocked(b, 1, NORMAL));
	return 0;
}

static int run_plan(int argc, char** argv)
{
	const char* levels = argv[2];
	int level_number = atoi(argv[3]);
	int strategy_index = atoi(argv[4]);
	int log_nodes = atoi(argv[5]);
	int backward_search_type[8] =
	{
		BASE_SEARCH,
		MAX_DIST_SEARCH2,
		GIRL_SEARCH,
		HF_SEARCH,
		BICON_SEARCH,
		MAX_DIST_SEARCH,
		REV_SEARCH,
		DRAGONFLY
	};
	tree *t;
	helper *h;
	board start;
	board b;

	if (strategy_index < 0 || strategy_index >= 8)
	{
		fprintf(stderr, "strategy must be 0..7\n");
		return 2;
	}
	if (backward_search_type[strategy_index] == REV_SEARCH ||
		backward_search_type[strategy_index] == GIRL_SEARCH ||
		backward_search_type[strategy_index] == DRAGONFLY)
	{
		fprintf(stderr, "strategy has no packing plan\n");
		return 2;
	}

	verbose = 0;
	cores_num = 1;
	time_limit = 3600;
	start_time = (int)time(0);
	setvbuf(stdout, NULL, _IONBF, 0);

	strcpy(global_level_set_name, levels);
	load_level_from_file(start, level_number);
	if (preprocess_level(start) == 0)
	{
		fprintf(stderr, "level %d failed preprocessing\n", level_number);
		return 1;
	}

	// The tree size is the fixed expansion budget for the comparison; the
	// perimeter and deadlock caches keep the default solver sizes.
	extra_mem = log_nodes - 23;
	allocate_search_trees();
	allocate_helpers();
	t = search_trees;
	h = helpers;
	h->weighted_search = 1;

	copy_board(start, b);
	packing_search(b, time_limit, backward_search_type[strategy_index], t, h);

	printf("expansions %d\n", t->expansions_num);
	printf("plan %d\n", h->parking_order_num);
	for (int i = 0; i < h->parking_order_num; i++)
		printf("%d %d %d\n",
			h->parking_order[i].from,
			h->parking_order[i].to,
			h->parking_order[i].until);

	free_helpers();
	free_search_trees();
	return 0;
}

static int load_parking_plan(const char* path, helper* h)
{
	FILE* fp;
	char line[2048];
	int count = 0;

	fp = fopen(path, "rb");
	if (fp == NULL)
	{
		fprintf(stderr, "can't open plan %s\n", path);
		return -1;
	}

	while (fgets(line, sizeof(line), fp) != NULL)
	{
		int from, to, until;
		if (sscanf(line, "%d %d %d", &from, &to, &until) != 3)
			continue;
		if (count >= MAX_SOL_LEN)
		{
			fprintf(stderr, "plan too long\n");
			fclose(fp);
			return -1;
		}
		h->parking_order[count].from = from;
		h->parking_order[count].to = to;
		h->parking_order[count].until = until;
		count++;
	}
	fclose(fp);
	h->parking_order_num = count;
	return 0;
}

static int run_dragonfly_stats(int argc, char** argv)
{
	const char* levels = argv[2];
	int level_number = atoi(argv[3]);
	int seconds = atoi(argv[4]);
	board start;
	helper h;

	verbose = 0;
	cores_num = 1;
	extra_mem = 0;
	time_limit = 600;
	start_time = (int)time(0);

	strcpy(global_level_set_name, levels);
	load_level_from_file(start, level_number);
	if (preprocess_level(start) == 0)
	{
		fprintf(stderr, "level %d failed preprocessing\n", level_number);
		return 1;
	}

	init_dragonfly();
	init_helper(&h);
	dragonfly_search(start, seconds, &h);

	printf("nodes %d\n", dragonfly_nodes_num);
	printf("roots %d\n", dragonfly_roots_num);
	printf("max_nodes %d\n", dragonfly_max_nodes);
	printf("perimeter_entries %d\n", total_perimeter_entries);
	printf("perimeter_full %d\n", perimeter_is_full());
	printf("perimeter_found %d\n", h.perimeter_found);
	if (argc >= 6 && strcmp(argv[5], "dump") == 0)
	{
		int i;
		for (i = 0; i < dragonfly_nodes_num; i++)
		{
			board b;
			dragonfly_node* e = dragonfly_nodes + i;
			dragonfly_get_board(e, b);
			printf("node %d %d %d %d %d %llu\n", i, e->depth, e->father,
				e->dist, e->packed, (unsigned long long)get_board_hash(b));
		}
	}
	free_helper(&h);
	return 0;
}

static int run_girl_score(int argc, char** argv)
{
	const char* levels = argv[2];
	int level_number = atoi(argv[3]);
	const char* plan_path = argv[4];
	const char* boards_path = argv[5];
	const char* out_path = argv[6];
	board start;
	helper h;
	FILE* fb;
	FILE* fo;
	char line[8192];
	int written = 0;

	verbose = 0;
	cores_num = 1;
	extra_mem = 0;
	time_limit = 600;
	start_time = (int)time(0);

	strcpy(global_level_set_name, levels);
	load_level_from_file(start, level_number);
	if (preprocess_level(start) == 0)
	{
		fprintf(stderr, "level %d failed preprocessing\n", level_number);
		return 1;
	}

	init_helper(&h);
	init_helper_extra_fields(&h);

	if (load_parking_plan(plan_path, &h) != 0)
		return 1;

	verify_parking_order(&h);
	reduce_parking_order(&h);

	fb = fopen(boards_path, "rb");
	fo = fopen(out_path, "wb");
	if (fb == NULL || fo == NULL)
	{
		fprintf(stderr, "can't open boards/output\n");
		return 1;
	}

	while (fgets(line, sizeof(line), fb) != NULL)
	{
		unsigned long long hash;
		char hex[8192];
		board b;
		score_element push_score;
		score_element pull_score;
		double push_value;
		double pull_value;
		int y, x;

		if (sscanf(line, "%llu %8191s", &hash, hex) != 2)
			continue;

		memset(b, 0, sizeof(board));
		for (y = 0; y < height; y++)
			for (x = 0; x < width; x++)
			{
				unsigned int value;
				sscanf(hex + 2 * (y * width + x), "%02x", &value);
				b[y][x] = (unsigned char)value;
			}

		score_board(b, &push_score, 0, GIRL_SEARCH, &h);
		score_board(b, &pull_score, 1, GIRL_SEARCH, &h);
		push_value = score_to_value(&push_score, 0);
		pull_value = score_to_value(&pull_score, 1);

		fprintf(fo, "%llu %d %d %d %d %d %.17e %d %d %d %.17e\n",
			hash,
			push_score.packed_boxes,
			push_score.boxes_on_targets,
			push_score.connectivity,
			push_score.dist_to_targets,
			push_score.overlap,
			push_value,
			pull_score.boxes_on_targets,
			pull_score.connectivity,
			pull_score.dist_to_targets,
			pull_value);
		written++;
	}

	fclose(fb);
	fclose(fo);
	free_helper(&h);
	printf("level %d: %d girl score rows\n", level_number, written);
	return 0;
}

int main(int argc, char** argv)
{
	// The solver's main() processes -cores/-extra_mem before allocating the
	// perimeter and caches; refcheck has no such flags, so fix them here.
	cores_num = 1;
	extra_mem = 0;

	if (argc >= 5 && strcmp(argv[1], "explain") == 0)
	{
		allocate_perimeter();
		allocate_deadlock_cache();
		read_deadlock_patterns(0);
		read_deadlock_patterns(1);
		return run_explain(argc, argv);
	}

	if (argc >= 8 && strcmp(argv[1], "deadlock") == 0)
	{
		allocate_perimeter();
		allocate_deadlock_cache();
		read_deadlock_patterns(0);
		read_deadlock_patterns(1);
		return run_deadlock(argc, argv);
	}

	if (argc >= 6 && strcmp(argv[1], "plan") == 0)
	{
		allocate_perimeter();
		allocate_deadlock_cache();
		read_deadlock_patterns(0);
		read_deadlock_patterns(1);
		return run_plan(argc, argv);
	}

	if (argc >= 5 && strcmp(argv[1], "dragonfly") == 0)
	{
		allocate_perimeter();
		allocate_deadlock_cache();
		read_deadlock_patterns(0);
		read_deadlock_patterns(1);
		return run_dragonfly_stats(argc, argv);
	}

	if (argc >= 7 && strcmp(argv[1], "girl") == 0)
	{
		allocate_perimeter();
		allocate_deadlock_cache();
		read_deadlock_patterns(0);
		read_deadlock_patterns(1);
		return run_girl_score(argc, argv);
	}

	fprintf(stderr,
		"usage: refcheck deadlock <levels.sok> <level> <samples> <move_samples> <seed> <out_prefix>\n"
		"       refcheck plan <levels.sok> <level> <strategy> <log_nodes>\n"
		"       refcheck girl <levels.sok> <level> <plan> <boards> <out>\n"
		"       refcheck dragonfly <levels.sok> <level> <seconds>\n");
	return 2;
}
