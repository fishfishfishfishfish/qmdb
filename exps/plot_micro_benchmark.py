import os
import argparse
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='Process some CSV files.')
    parser.add_argument('db_name', type=str, help='The name of the tested db')
    parser.add_argument('test_name', type=str, help='The name of the test')
    args = parser.parse_args()

    detail_dir = f'results_{args.db_name}/{args.test_name}'
    summary_file = f'results_{args.db_name}/{args.test_name}_summary.csv'
    range_file = f'results_{args.db_name}/{args.test_name}_range.csv'
    summary_plot_file = f'results_{args.db_name}/{args.test_name}_summary.png'
    
    
    summary_dict = {"entry_count":[], "batch_size":[], "value_size":[], 
                    "load_latency":[], "get_latency":[], "put_latency":[],
                    "load_throughput":[], "get_throughput":[], "put_throughput":[]}
    range_dict = {"entry_count":[], "batch_size":[], "value_size":[], "range_size": [], 
                  "latency":[], "throughput":[]}
    detail_files = []
    for entry in os.listdir(detail_dir):
        full_path = os.path.join(detail_dir, entry)
        if os.path.isfile(full_path) and entry.endswith('.csv'):
            fname = full_path.split('/')[-1].split('.')[0]
            fname, vl = fname.split('v')
            vl = int(vl)
            acc, bz = fname.split('b')
            acc, bz = int(acc.strip('e')), int(bz)
            detail_files.append((acc, bz, vl, fname, full_path))
    detail_files = sorted(detail_files)
    for acc, bz, vl, fn, fp in detail_files:
        df = pd.read_csv(fp)
        load_df = df[df['operation'] == 'LOAD']
        put_df = df[df['operation'] == 'PUT']
        get_df = df[df['operation'] == 'GET']
        
        summary_dict["entry_count"].append(acc)
        summary_dict["batch_size"].append(bz)
        summary_dict["value_size"].append(vl)
        
        summary_dict["load_latency"].append(np.mean(load_df['latency'].to_numpy()))
        summary_dict["get_latency"].append(np.mean(get_df['latency'].to_numpy()))
        summary_dict["put_latency"].append(np.mean(put_df['latency'].to_numpy()))
        summary_dict["load_throughput"].append(np.mean(load_df['throughput'].to_numpy()))
        summary_dict["get_throughput"].append(np.mean(get_df['throughput'].to_numpy()))
        summary_dict["put_throughput"].append(np.mean(put_df['throughput'].to_numpy()))
        
        # ranges = df[df['operation'].str.startswith('RANGE')]['operation'].unique()
        # for r in ranges:
        #     data = df[df['operation'] == r]
        #     range_dict["entry_count"].append(acc)
        #     range_dict["batch_size"].append(bz)
        #     range_dict["value_size"].append(vl)
        #     range_dict["range_size"].append(int(r.strip('RANGE')))
        #     range_dict["throughput"].append(np.mean(data['throughput'].to_numpy()))
        #     range_dict["latency"].append(np.mean(data['latency'].to_numpy()))
        
    summary_df =  pd.DataFrame(summary_dict)
    summary_df.to_csv(summary_file, index=False)