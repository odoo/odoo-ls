import { final_answer } from "./greeting_report";

const half_answer = 21;

export function answer_is_correct(answer) {
    if (answer < half_answer) return false;
    return answer === final_answer();
}
