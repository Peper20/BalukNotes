// Глава 1: независимый расчёт ожидаемого результата (оракул).
// Сборка и тест: g++ -std=c++20 -Wall -Wextra -fsanitize=address,undefined стресс.cpp && ./a.out  → OK
#include <bits/stdc++.h>
#include <cassert>
using namespace std;

// тестируемая функция: сортировка вставками
void my_sort(vector<int>& a) {
    for (size_t i = 1; i < a.size(); ++i)
        for (size_t j = i; j > 0 && a[j - 1] > a[j]; --j) swap(a[j - 1], a[j]);
}

int check() {
// region: stress
mt19937 rng(42);                   // фиксированный seed
for (int iter = 0; iter < 100000; ++iter) {
    int n = rng() % 8;             // маленькие входы
    vector<int> a(n);
    for (int& x : a) x = int(rng() % 11) - 5;  // много повторов
    vector<int> got = a, want = a;
    my_sort(got);
    sort(want.begin(), want.end());          // оракул 1: эталон
    bool props = is_sorted(got.begin(), got.end())  // оракул 2
              && is_permutation(got.begin(), got.end(), a.begin());
    if (got != want || !props) {
        cout << "FAIL, iter " << iter << ", input:";
        for (int x : a) cout << ' ' << x;
        return 1;
    }
}
// endregion: stress
return 0;
}

int main() {
    assert(check() == 0);
    puts("OK");
}
