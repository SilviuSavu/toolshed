package main

import (
	"context"
	"database/sql"
	"fmt"
	"log"
	"os"
	"sync"
	"time"
)

type TestCase struct {
	BuildOptions map[string]string
	Name         string
	Endpoints    Endpoints
	Count        int
	QueryTrigger QueryTrigger
	RawQuery     string
}

func (t *TestCase) Query() string {
	return t.RawQuery
}

type QueryTrigger struct {
	Count    int
	Interval time.Duration
}

func (q *QueryTrigger) C() <-chan time.Time {
	c := make(chan time.Time, 1000)
	c <- time.Now()
	go func() {
		timer := time.NewTicker(q.Interval)
		defer timer.Stop()
		defer close(c)
		for i := 0; i < q.Count-1; i++ {
			t := <-timer.C
			c <- t
		}
	}()
	return c
}

func (q *QueryTrigger) ProfileTime() time.Duration {
	return time.Duration(q.Count)*q.Interval + 5*time.Second
}

type Endpoints struct {
	FrontendEndpoint string
	Token            string
}

type result struct {
	Took        int64
	ResultCount int
	Err         error
}

func collectResults(tc *TestCase, client *client, mc chan *result) {
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()

	results, metrics, err := client.search(ctx, tc.Query())
	if err != nil {
		log.Printf("  [%s] ERROR: %s", tc.Name, err)
		mc <- &result{Err: err}
		return
	}
	log.Printf("  [%s] %d results in %dms", tc.Name, results.Search.Results.ResultCount, metrics.took)
	mc <- &result{
		Took:        metrics.took,
		ResultCount: results.Search.Results.ResultCount,
	}
}

func runTest(tc *TestCase, db *sql.DB) {
	log.Printf("=== %s ===", tc.Name)
	log.Printf("    query: %s", tc.Query())

	if err := insertTest(db, tc); err != nil {
		log.Fatalf("Insert test case: %s", err)
	}

	client, err := newClient(tc.Endpoints.FrontendEndpoint, tc.Endpoints.Token)
	if err != nil {
		log.Fatalf("Failed to create client: %s", err)
	}

	var wg sync.WaitGroup
	mc := make(chan *result, 1000)

	var resultsWg sync.WaitGroup
	for range tc.QueryTrigger.C() {
		resultsWg.Add(1)
		go func() {
			collectResults(tc, client, mc)
			resultsWg.Done()
		}()
	}

	wg.Add(1)
	go func() {
		resultsWg.Wait()
		close(mc)
		wg.Done()
	}()

	wg.Add(1)
	go func() {
		for result := range mc {
			if err := insertResult(db, tc, result); err != nil {
				log.Fatalf("Insert result: %s", err)
			}
		}
		wg.Done()
	}()

	wg.Wait()
}

func main() {
	endpoint := os.Getenv("SG_ENDPOINT")
	if endpoint == "" {
		endpoint = "http://localhost:7080"
	}
	token := os.Getenv("SG_TOKEN")
	if token == "" {
		log.Fatal("SG_TOKEN environment variable is required")
	}

	ep := Endpoints{FrontendEndpoint: endpoint, Token: token}

	// 5 runs, 1s apart — gives us p50/p95/p99 with 5 data points per query
	trigger := QueryTrigger{Count: 5, Interval: 1 * time.Second}

	cases := []*TestCase{
		// ===== Keyword search =====
		{Name: "keyword_small", RawQuery: "func main lang:rust count:10", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "keyword", "size": "small"}},
		{Name: "keyword_medium", RawQuery: "error handling lang:rust count:100", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "keyword", "size": "medium"}},
		{Name: "keyword_large", RawQuery: "use count:1000", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "keyword", "size": "large"}},

		// ===== Regex search =====
		{Name: "regex_simple", RawQuery: `fn\s+\w+\(.*Result lang:rust count:30`, Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "regex", "size": "medium"}},
		{Name: "regex_complex", RawQuery: `unsafe\s*\{[^}]*\} lang:rust count:50`, Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "regex", "size": "large"}},

		// ===== Symbol search =====
		{Name: "symbol_function", RawQuery: "type:symbol run kind:function count:30", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "symbol", "size": "medium"}},
		{Name: "symbol_struct", RawQuery: "type:symbol Config kind:class count:20", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "symbol", "size": "small"}},

		// ===== Commit search =====
		{Name: "commit_recent", RawQuery: "type:commit repo:gitlab/root/g3 count:10", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "commit", "size": "small"}},
		{Name: "commit_pattern", RawQuery: "type:commit fix count:20", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "commit", "size": "medium"}},

		// ===== Diff search =====
		{Name: "diff_unsafe", RawQuery: "type:diff unsafe lang:rust count:10", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "diff", "size": "small"}},
		{Name: "diff_broad", RawQuery: "type:diff TODO count:30", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "diff", "size": "medium"}},

		// ===== Repo-scoped search =====
		{Name: "repo_scoped_g3", RawQuery: "repo:gitlab/root/g3 async count:50", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "repo_scoped", "size": "medium"}},
		{Name: "repo_scoped_all", RawQuery: "repo:gitlab/root/ fn count:100", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "repo_scoped", "size": "large"}},

		// ===== File listing =====
		{Name: "file_list", RawQuery: "repo:gitlab/root/g3 type:path count:100", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "file", "size": "large"}},

		// ===== Select repo =====
		{Name: "select_repo", RawQuery: "repo:gitlab/root/ select:repo count:10", Endpoints: ep, QueryTrigger: trigger,
			BuildOptions: map[string]string{"category": "select", "size": "small"}},
	}

	dir := fmt.Sprintf("run_%s", time.Now().Format("2006-01-02_15-04-05"))
	if err := os.Mkdir(dir, 0755); err != nil {
		log.Fatalf("Failed to create results dir: %s", err)
	}

	db, err := sql.Open("sqlite3", dir+"/results.sqlite")
	if err != nil {
		log.Fatalf("Open db: %s", err)
	}
	if err := Initialize(db); err != nil {
		log.Fatalf("Initialize db: %s", err)
	}

	log.Printf("Sourcegraph Benchmark — %d test cases", len(cases))
	log.Printf("Endpoint: %s", endpoint)
	log.Printf("Results: %s/results.sqlite", dir)
	log.Println()

	for i, tc := range cases {
		log.Printf("[%d/%d] Running %s", i+1, len(cases), tc.Name)
		runTest(tc, db)
		log.Println()
	}

	// Print summary
	printSummary(db)
}

func printSummary(db *sql.DB) {
	log.Println("============================================================")
	log.Println("                    BENCHMARK RESULTS")
	log.Println("============================================================")
	fmt.Printf("%-25s %8s %8s %8s %8s %6s\n", "TEST", "AVG(ms)", "MIN(ms)", "MAX(ms)", "P95(ms)", "COUNT")
	fmt.Println("-------------------------------------------------------------------")

	rows, err := db.Query(`
		SELECT
			r.test_case,
			ROUND(AVG(r.took), 0) as avg_ms,
			MIN(r.took) as min_ms,
			MAX(r.took) as max_ms,
			r.took as p95_ms,
			COUNT(*) as cnt,
			COUNT(r.error) as errors
		FROM results r
		WHERE r.error IS NULL
		GROUP BY r.test_case
		ORDER BY r.test_case
	`)
	if err != nil {
		log.Printf("Query error: %s", err)
		return
	}
	defer rows.Close()

	for rows.Next() {
		var name string
		var avgMs, minMs, maxMs, p95Ms float64
		var cnt, errors int
		if err := rows.Scan(&name, &avgMs, &minMs, &maxMs, &p95Ms, &cnt, &errors); err != nil {
			log.Printf("Scan error: %s", err)
			continue
		}
		errStr := ""
		if errors > 0 {
			errStr = fmt.Sprintf(" (%d err)", errors)
		}
		fmt.Printf("%-25s %8.0f %8.0f %8.0f %8.0f %6d%s\n", name, avgMs, minMs, maxMs, p95Ms, cnt, errStr)
	}
	fmt.Println("-------------------------------------------------------------------")
}
