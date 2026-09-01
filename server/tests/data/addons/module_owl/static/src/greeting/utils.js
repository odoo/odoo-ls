import { final_answer } from "./greeting_report";
import { useChildRef } from "@web/core/utils/hooks";

const half_answer = 21;

export function answer_is_correct(answer) {
    if (answer < half_answer) return false;
    return answer === final_answer();
}

export function useGreetingRefs() {
    const ref = useChildRef();
    const service = useS;
    return { ref, service };
}
