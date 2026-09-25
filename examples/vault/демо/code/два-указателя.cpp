// Демо для код-из-файла: пара с заданной суммой в отсортированном массиве.
// Сборка и тест: g++ -std=c++20 два-указателя.cpp && ./a.out  → OK
#include <bits/stdc++.h>
#include <cassert>
using namespace std;

// region: pair
// индексы пары с суммой s или (-1, -1); a отсортирован по возрастанию
pair<int, int> find_pair(const vector<int>& a, int s) {
    int l = 0, r = (int)a.size() - 1;
    while (l < r) {
        int cur = a[l] + a[r];
        if (cur == s) return {l, r};
        if (cur < s) ++l;   // нужна сумма больше — двигаем левый
        else --r;           // нужна меньше — двигаем правый
    }
    return {-1, -1};
}
// endregion: pair

int main() {
    vector<int> a = {1, 2, 4, 5, 7, 9, 11};
    assert(find_pair(a, 14) == make_pair(3, 5));
    assert(find_pair(a, 100) == make_pair(-1, -1));
    puts("OK");
}
